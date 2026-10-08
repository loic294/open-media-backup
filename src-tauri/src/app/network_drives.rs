use super::{AppCore, AppSettings};
use crate::domain::{Destination, DestinationKind, Device, DeviceKind, DeviceMapping};
use std::path::Path;

fn validate_share(value: &str) -> Result<reqwest::Url, String> {
    let url = reqwest::Url::parse(value).map_err(|_| "Invalid stored network drive")?;
    if url.scheme() != "smb"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path().trim_matches('/').is_empty()
    {
        return Err("Invalid stored network drive".into());
    }
    Ok(url)
}

/// macOS mount reports SMB sources as //user@host/share; never retain userinfo.
fn share_for_mount(output: &str, root: &Path) -> Option<String> {
    output.lines().find_map(|line| {
        let (mount, options) = line.rsplit_once(" (")?;
        if !options.starts_with("smbfs,") {
            return None;
        }
        let (source, mount_path) = mount.split_once(" on ")?;
        if !root.starts_with(Path::new(mount_path)) {
            return None;
        }
        let source = source.strip_prefix("//")?;
        let (authority, share) = source.split_once('/')?;
        let host = authority.rsplit('@').next()?;
        validate_share(&format!("smb://{host}/{share}"))
            .ok()
            .map(|url| url.to_string())
    })
}

fn connect_command(share: &str) -> Result<std::process::Command, String> {
    let url = validate_share(share)?;
    let mut command = std::process::Command::new("/usr/bin/open");
    command.args(["-a", "Finder", url.as_str()]);
    Ok(command)
}

impl AppCore {
    pub fn remember_network_drives(&self) -> Result<(), String> {
        if !cfg!(target_os = "macos") {
            return Ok(());
        }
        let output = std::process::Command::new("/sbin/mount")
            .args(["-t", "smbfs"])
            .output()
            .map_err(|_| "Could not inspect mounted network drives")?;
        if !output.status.success() {
            return Ok(());
        }
        self.remember_network_mounts(&String::from_utf8_lossy(&output.stdout))
    }

    fn remember_network_mounts(&self, output: &str) -> Result<(), String> {
        let _save = self.settings_save.lock();
        let mut settings = AppSettings::load(&self.store);
        let before = settings.network_drives.clone();
        let devices = self.store.list::<Device>().map_err(|e| e.to_string())?;
        let mappings = self
            .store
            .list::<DeviceMapping>()
            .map_err(|e| e.to_string())?;
        for mapping in mappings {
            if mapping.computer_id == self.store.computer_id()
                && devices
                    .iter()
                    .any(|d| d.id == mapping.device_id && d.kind == DeviceKind::Nas)
            {
                if let Some(share) = share_for_mount(output, Path::new(&mapping.root_path)) {
                    settings.network_drives.insert(mapping.device_id, share);
                }
            }
        }
        if settings.network_drives != before {
            settings.save(&self.store).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    fn destination_network_drive(&self, destination_id: &str) -> Result<String, String> {
        let destination = self
            .store
            .get::<Destination>(destination_id)
            .map_err(|e| e.to_string())?
            .ok_or("Destination not found")?;
        let device = self
            .store
            .get::<Device>(&destination.device_id)
            .map_err(|e| e.to_string())?
            .ok_or("Device not found")?;
        if destination.kind != DestinationKind::Folder || device.kind != DeviceKind::Nas {
            return Err("Only NAS folder destinations can connect a network drive".into());
        }
        let share = AppSettings::load(&self.store).network_drives
            .get(&device.id).cloned()
            .ok_or("Connect this NAS once in Finder while Open Media Backup is running to remember its network drive on this Mac")?;
        validate_share(&share)?;
        Ok(share)
    }

    pub fn connect_destination_network_drive(&self, destination_id: &str) -> Result<(), String> {
        if !cfg!(target_os = "macos") {
            return Err("Connecting network drives is only supported on macOS".into());
        }
        let share = self.destination_network_drive(destination_id)?;
        let output = connect_command(&share)?
            .output()
            .map_err(|_| "Could not open the network drive in macOS")?;
        if !output.status.success() {
            return Err("macOS could not start connecting the network drive".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{app::DeviceResolver, store::Store};
    use std::sync::Arc;

    fn core() -> AppCore {
        let store = Arc::new(Store::open_in_memory().unwrap());
        AppCore::new(
            store.clone(),
            Arc::new(DeviceResolver(store)),
            ".".into(),
            |_| {},
        )
    }

    #[test]
    fn discovers_share_without_credentials_and_matches_path_components() {
        let mounts = "//user:secret@NAS.local/Photo%20Archive on /Volumes/Photo Archive (smbfs, nodev, mounted by user)\n";
        assert_eq!(
            share_for_mount(mounts, Path::new("/Volumes/Photo Archive/Backups")),
            Some("smb://NAS.local/Photo%20Archive".into())
        );
        assert_eq!(
            share_for_mount(mounts, Path::new("/Volumes/Photo Archives")),
            None
        );
        assert_eq!(
            share_for_mount(mounts, Path::new("/volumes/photo archive")),
            None
        );
        assert_eq!(
            share_for_mount(
                "/dev/disk1 on /Volumes/Photo Archive (apfs, local)",
                Path::new("/Volumes/Photo Archive")
            ),
            None
        );
        assert_eq!(
            share_for_mount(
                "//nas/Photo Archive on /Volumes/Photo Archive (smbfs, nodev)",
                Path::new("/Volumes/Photo Archive")
            ),
            Some("smb://nas/Photo%20Archive".into())
        );
        assert_eq!(
            share_for_mount(
                "//nas/Photos on /Volumes/Photos (smbfs, nodev)",
                Path::new("/Volumes/Missing")
            ),
            None
        );
        assert_eq!(
            share_for_mount(
                "//user@ on /Volumes/Photos (smbfs, nodev)",
                Path::new("/Volumes/Photos")
            ),
            None
        );
    }

    #[test]
    fn validates_urls_and_uses_fixed_executable_and_separate_arguments() {
        for value in [
            "file:///bin/sh",
            "smb://user:secret@nas/share",
            "smb://nas/",
            "smb://nas/share?password=secret",
            "smb://nas/share#fragment",
            "-a Calculator",
        ] {
            assert!(connect_command(value).is_err(), "{value}");
        }
        let command = connect_command("smb://nas/Photo Archive").unwrap();
        assert_eq!(command.get_program(), "/usr/bin/open");
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            ["-a", "Finder", "smb://nas/Photo%20Archive"]
        );
    }

    #[test]
    fn remembers_only_local_nas_and_preserves_backend_owned_settings() {
        let core = core();
        for (id, kind, computer) in [
            ("nas", DeviceKind::Nas, core.store.computer_id()),
            ("ssd", DeviceKind::Ssd, core.store.computer_id()),
            ("remote", DeviceKind::Nas, "peer"),
        ] {
            core.store
                .put(&Device {
                    id: id.into(),
                    kind,
                    ..Default::default()
                })
                .unwrap();
            core.store
                .put(&DeviceMapping {
                    id: format!("{id}@{computer}"),
                    device_id: id.into(),
                    computer_id: computer.into(),
                    root_path: "/Volumes/Photos".into(),
                })
                .unwrap();
        }
        core.remember_network_mounts("//user:secret@nas/Photos on /Volumes/Photos (smbfs, nodev)")
            .unwrap();
        let settings = core.settings();
        assert_eq!(settings.network_drives.len(), 1);
        assert_eq!(settings.network_drives["nas"], "smb://nas/Photos");
        let json = serde_json::to_string(&settings).unwrap();
        assert!(!json.contains("user"));
        assert!(!json.contains("secret"));
        let mut incoming = settings.clone();
        incoming
            .network_drives
            .insert("nas".into(), "file:///bin/sh".into());
        core.save_settings(&incoming).unwrap();
        assert_eq!(core.settings().network_drives, settings.network_drives);
        core.remember_network_mounts("").unwrap();
        assert_eq!(core.settings().network_drives, settings.network_drives);
        core.store
            .put(&Destination {
                id: "dest".into(),
                device_id: "nas".into(),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(
            core.destination_network_drive("dest").unwrap(),
            "smb://nas/Photos"
        );
        assert!(core.destination_network_drive("missing").is_err());
        core.store
            .put(&Destination {
                id: "ssd-dest".into(),
                device_id: "ssd".into(),
                ..Default::default()
            })
            .unwrap();
        assert!(core.destination_network_drive("ssd-dest").is_err());
        core.store
            .put(&Destination {
                id: "app".into(),
                device_id: "nas".into(),
                kind: DestinationKind::App,
                ..Default::default()
            })
            .unwrap();
        assert!(core.destination_network_drive("app").is_err());
        core.store
            .put(&Destination {
                id: "remote-dest".into(),
                device_id: "remote".into(),
                ..Default::default()
            })
            .unwrap();
        assert!(core
            .destination_network_drive("remote-dest")
            .unwrap_err()
            .contains("Finder"));
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn unsupported_platform_does_not_launch() {
        assert!(core()
            .connect_destination_network_drive("anything")
            .unwrap_err()
            .contains("only supported on macOS"));
    }
}
