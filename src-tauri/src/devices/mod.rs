mod identify;
mod marker;
#[cfg(target_os = "macos")]
mod probe_macos;
#[cfg(target_os = "windows")]
mod probe_windows;
mod registry;
mod volumes;
#[cfg(test)]
mod volumes_tests;
mod watcher;

pub use marker::{read_marker, write_marker, Marker, MARKER_DIR};
pub use registry::{register_device, relink_device, resolve_root};
pub use volumes::{list_volumes, VolumeInfo, VolumeMatch};
pub use watcher::spawn_watcher;

#[derive(Debug, thiserror::Error)]
pub enum DeviceError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("store error: {0}")]
    Store(#[from] crate::store::StoreError),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("device not found: {0}")]
    NotFound(String),
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct ProbeInfo {
    pub mount_path: String,
    pub name: String,
    pub volume_uuid: Option<String>,
    pub hw_serial: Option<String>,
    pub total_bytes: Option<u64>,
    pub free_bytes: Option<u64>,
    pub removable: bool,
}
