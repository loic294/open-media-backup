use super::{entity::impl_entity, FileRule};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceRole {
    #[default]
    Original,
    Temporary,
    Final,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceKind {
    SdCard,
    Ssd,
    Hdd,
    Nas,
    Computer,
    Camera,
    Drone,
    #[default]
    Other,
}

/// A storage device known to every peer, independent of where it is mounted.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub description: String,
    pub role: DeviceRole,
    pub kind: DeviceKind,
    pub hw_serial: Option<String>,
    pub volume_uuid: Option<String>,
    pub capacity_bytes: Option<u64>,
    /// Shared safe-copy rules, evaluated relative to each source folder on this device.
    pub safe_copy_rules: Vec<FileRule>,
}
impl_entity!(Device, Device);

/// Where a device is reachable on a given computer (e.g. `/Volumes/CAMERA_A` or `E:\`).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DeviceMapping {
    pub id: String,
    pub device_id: String,
    pub computer_id: String,
    pub root_path: String,
}
impl_entity!(DeviceMapping, DeviceMapping);

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Computer {
    pub id: String,
    pub name: String,
    pub os: String,
}
impl_entity!(Computer, Computer);
