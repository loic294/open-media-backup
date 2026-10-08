use crate::domain::Device;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Volume labels are limited (exFAT: 11 chars on macOS diskutil, 15 on Windows).
fn label(name: &str) -> String {
    let clean: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == ' ' || *c == '_')
        .collect();
    let trimmed: String = clean.trim().chars().take(11).collect();
    if trimmed.is_empty() {
        "MEDIA".into()
    } else {
        trimmed.to_uppercase()
    }
}

fn device_label(device: &Device) -> String {
    let name = device.format_name.trim();
    label(if name.is_empty() { &device.name } else { name })
}

fn run(mut cmd: Command) -> Result<(), String> {
    let output = cmd
        .output()
        .map_err(|e| format!("could not start formatter: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "format failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

pub(super) enum FormattedVolume {
    MacDevice(String),
    MountPoint(PathBuf),
}

impl FormattedVolume {
    pub(super) fn mounted_root(self) -> Result<PathBuf, String> {
        match self {
            Self::MacDevice(identifier) => {
                diskutil_info(&identifier, "MountPoint").map(PathBuf::from)
            }
            Self::MountPoint(root) => Ok(root),
        }
    }
}

fn diskutil_info(target: &str, field: &str) -> Result<String, String> {
    let output = Command::new("diskutil")
        .args(["info", "-plist", target])
        .output()
        .map_err(|e| format!("could not inspect format volume: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "could not inspect format volume: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    diskutil_field(&output.stdout, field)
}

fn diskutil_field(bytes: &[u8], field: &str) -> Result<String, String> {
    let info = plist::Value::from_reader_xml(bytes)
        .map_err(|e| format!("invalid format volume information: {e}"))?;
    info.as_dictionary()
        .and_then(|dict| dict.get(field))
        .and_then(plist::Value::as_string)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| format!("format volume information is missing {field}"))
}

/// Erases the volume as exFAT. Resolve its mount point afterward: macOS may rename it.
pub(super) fn quick_format(root: &Path, device: &Device) -> Result<FormattedVolume, String> {
    if cfg!(target_os = "macos") {
        if !root.starts_with("/Volumes/") || root.components().count() != 3 {
            return Err("refusing to format something that is not an external volume".into());
        }
        let target = root.to_str().ok_or("volume path is not valid UTF-8")?;
        let identifier = diskutil_info(target, "DeviceIdentifier")?;
        let mut cmd = Command::new("diskutil");
        cmd.args(["eraseVolume", "ExFAT", &device_label(device)])
            .arg(&identifier);
        run(cmd)?;
        Ok(FormattedVolume::MacDevice(identifier))
    } else if cfg!(target_os = "windows") {
        let letter = root
            .to_string_lossy()
            .chars()
            .next()
            .filter(|c| c.is_ascii_alphabetic())
            .ok_or("no drive letter")?;
        if letter.eq_ignore_ascii_case(&'C') {
            return Err("refusing to format the system drive".into());
        }
        let script = format!(
            "Format-Volume -DriveLetter {letter} -FileSystem exFAT -NewFileSystemLabel '{}' -Force -Confirm:$false",
            device_label(device)
        );
        let mut cmd = Command::new("powershell");
        cmd.args(["-NoProfile", "-NonInteractive", "-Command", &script]);
        run(cmd)?;
        Ok(FormattedVolume::MountPoint(root.to_path_buf()))
    } else {
        Err("quick format is not supported on this platform".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_name_labels_are_safe() {
        assert_eq!(super::label("CAM1 Card #1 extra"), "CAM1 CARD 1");
        assert_eq!(super::label("ü/"), "MEDIA");
    }

    #[test]
    fn format_name_overrides_display_name_only_for_formatting() {
        let mut device = Device {
            name: "Camera card".into(),
            format_name: " a7_iv ".into(),
            ..Default::default()
        };
        assert_eq!(device_label(&device), "A7_IV");
        assert_eq!(device.name, "Camera card");
        device.format_name = "   ".into();
        assert_eq!(device_label(&device), "CAMERA CARD");
        device.format_name.clear();
        assert_eq!(device_label(&device), "CAMERA CARD");
        device.format_name = "ü/'".into();
        assert_eq!(device_label(&device), "MEDIA");
    }

    #[test]
    fn format_name_mount_resolution_uses_disk_identifier_not_label() {
        let xml = br#"<?xml version="1.0"?><plist version="1.0"><dict>
            <key>DeviceIdentifier</key><string>disk4s1</string>
            <key>MountPoint</key><string>/Volumes/A7_IV 1</string>
            </dict></plist>"#;
        assert_eq!(diskutil_field(xml, "DeviceIdentifier").unwrap(), "disk4s1");
        assert_eq!(
            diskutil_field(xml, "MountPoint").unwrap(),
            "/Volumes/A7_IV 1"
        );
        assert!(diskutil_field(b"invalid", "MountPoint").is_err());
        let unmounted = br#"<?xml version="1.0"?><plist version="1.0"><dict>
            <key>DeviceIdentifier</key><string>disk4s1</string></dict></plist>"#;
        assert!(diskutil_field(unmounted, "MountPoint").is_err());
    }
}
