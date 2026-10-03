use crate::domain::Device;
use serde::{Deserialize, Serialize};
use std::{fs, io, path::Path};

pub const MARKER_DIR: &str = ".openmediabackup";
const MARKER_FILE: &str = "device.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Marker {
    pub device_id: String,
    pub name: String,
    pub written_at: i64,
}

pub fn read_marker(root: &Path) -> Option<Marker> {
    let path = root.join(MARKER_DIR).join(MARKER_FILE);
    let data = fs::read(path).ok()?;
    serde_json::from_slice(&data).ok()
}

pub fn write_marker(root: &Path, device: &Device) -> io::Result<()> {
    let dir = root.join(MARKER_DIR);
    fs::create_dir_all(&dir)?;
    let marker = Marker {
        device_id: device.id.clone(),
        name: device.name.clone(),
        written_at: chrono::Utc::now().timestamp_millis(),
    };
    let data = serde_json::to_vec_pretty(&marker).map_err(io::Error::other)?;
    fs::write(dir.join(MARKER_FILE), data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marker_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let device = Device {
            id: "dev1".into(),
            name: "Card".into(),
            ..Default::default()
        };
        write_marker(dir.path(), &device).unwrap();
        let marker = read_marker(dir.path()).unwrap();
        assert_eq!(marker.device_id, "dev1");
        assert_eq!(marker.name, "Card");
    }
}
