use super::ProbeInfo;
use serde_json::Value;
use std::{
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const TIMEOUT: Duration = Duration::from_secs(3);
const CREATE_NO_WINDOW: u32 = 0x08000000;

pub(crate) fn probe_path(path: &str) -> Option<ProbeInfo> {
    let root = path.chars().take(2).collect::<String>();
    let script = format!(
        "$v=Get-Volume -DriveLetter '{}' -ErrorAction SilentlyContinue; if($v){{$p=Get-Partition -DriveLetter '{}' -ErrorAction SilentlyContinue; $d=$p|Get-Disk -ErrorAction SilentlyContinue; [pscustomobject]@{{MountPath='{}:\\';Name=$v.FileSystemLabel;VolumeUuid=$v.UniqueId;HwSerial=$d.SerialNumber;TotalBytes=$v.Size;FreeBytes=$v.SizeRemaining;Removable=($v.DriveType -eq 'Removable')}}|ConvertTo-Json -Compress}}",
        root.chars().next()?, root.chars().next()?, root.chars().next()?
    );
    let out = run_with_timeout(Command::new("powershell").args([
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        &script,
    ]))?;
    parse_volume_json(std::str::from_utf8(&out).ok()?, path)
}

fn run_with_timeout(command: &mut Command) -> Option<Vec<u8>> {
    use std::os::windows::process::CommandExt;
    let mut child = command
        .creation_flags(CREATE_NO_WINDOW)
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

pub(crate) fn parse_volume_json(text: &str, fallback_path: &str) -> Option<ProbeInfo> {
    let value: Value = serde_json::from_str(text).ok()?;
    let value = value.as_array().and_then(|a| a.first()).unwrap_or(&value);
    let get_str = |key: &str| {
        value
            .get(key)
            .and_then(Value::as_str)
            .map(str::to_string)
            .filter(|s| !s.is_empty())
    };
    let get_u64 = |key: &str| value.get(key).and_then(Value::as_u64);
    Some(ProbeInfo {
        mount_path: get_str("MountPath").unwrap_or_else(|| fallback_path.to_string()),
        name: get_str("Name").unwrap_or_else(|| fallback_path.to_string()),
        volume_uuid: get_str("VolumeUuid"),
        hw_serial: get_str("HwSerial"),
        total_bytes: get_u64("TotalBytes"),
        free_bytes: get_u64("FreeBytes"),
        removable: value
            .get("Removable")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_volume_json() {
        let got = parse_volume_json(r#"{"MountPath":"E:\\","Name":"CARD","VolumeUuid":"vol","HwSerial":"ser","TotalBytes":10,"FreeBytes":4,"Removable":true}"#, "E:\\").unwrap();
        assert_eq!(got.name, "CARD");
        assert_eq!(got.hw_serial.as_deref(), Some("ser"));
        assert!(got.removable);
    }
}
