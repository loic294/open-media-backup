use super::ProbeInfo;
use plist::Value;
use std::{
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const TIMEOUT: Duration = Duration::from_secs(3);

pub(crate) fn probe_path(path: &str) -> Option<ProbeInfo> {
    let out = run_with_timeout(Command::new("diskutil").arg("info").arg("-plist").arg(path))?;
    parse_diskutil_plist(&out, path)
}

fn run_with_timeout(command: &mut Command) -> Option<Vec<u8>> {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if child.try_wait().ok()?.is_some() {
            return child
                .wait_with_output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| o.stdout);
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        thread::sleep(Duration::from_millis(25));
    }
}

pub(crate) fn parse_diskutil_plist(bytes: &[u8], fallback_path: &str) -> Option<ProbeInfo> {
    let value = Value::from_reader_xml(bytes).ok()?;
    let dict = value.as_dictionary()?;
    let get_str = |key: &str| {
        dict.get(key)
            .and_then(Value::as_string)
            .map(str::to_string)
            .filter(|s| !s.is_empty())
    };
    let get_bool = |key: &str| dict.get(key).and_then(Value::as_boolean).unwrap_or(false);
    let get_u64 = |key: &str| dict.get(key).and_then(Value::as_unsigned_integer);
    let name = get_str("VolumeName")
        .or_else(|| get_str("MediaName"))
        .or_else(|| get_str("DeviceIdentifier"))
        .unwrap_or_else(|| fallback_path.to_string());
    let removable = get_bool("Removable") || get_bool("RemovableMedia") || !get_bool("Internal");
    Some(ProbeInfo {
        mount_path: get_str("MountPoint").unwrap_or_else(|| fallback_path.to_string()),
        name,
        volume_uuid: get_str("VolumeUUID").or_else(|| get_str("DiskUUID")),
        hw_serial: get_str("MediaSerialNumber"),
        total_bytes: get_u64("TotalSize").or_else(|| get_u64("Size")),
        free_bytes: get_u64("FreeSpace"),
        removable,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_diskutil_plist() {
        let xml = br#"<?xml version="1.0" encoding="UTF-8"?><!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd"><plist version="1.0"><dict><key>MountPoint</key><string>/Volumes/CARD</string><key>VolumeName</key><string>CARD</string><key>VolumeUUID</key><string>vol</string><key>MediaSerialNumber</key><string>ser</string><key>TotalSize</key><integer>10</integer><key>FreeSpace</key><integer>4</integer><key>RemovableMedia</key><true/><key>Internal</key><false/></dict></plist>"#;
        let got = parse_diskutil_plist(xml, "/Volumes/CARD").unwrap();
        assert_eq!(got.mount_path, "/Volumes/CARD");
        assert_eq!(got.name, "CARD");
        assert_eq!(got.volume_uuid.as_deref(), Some("vol"));
        assert_eq!(got.hw_serial.as_deref(), Some("ser"));
        assert!(got.removable);
    }
}
