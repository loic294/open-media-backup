use super::{blocking, CmdResult, Shared};
use crate::devices::VolumeInfo;
use crate::domain::Device;
use tauri::State;

#[tauri::command]
pub async fn list_volumes(state: State<'_, Shared>) -> CmdResult<Vec<VolumeInfo>> {
    blocking(&state, |s| Ok(crate::devices::list_volumes(&s.core.store))).await
}

#[tauri::command]
pub async fn register_device(state: State<'_, Shared>, mount_path: String, device: Device) -> CmdResult<Device> {
    blocking(&state, move |s| {
        crate::devices::register_device(&s.core.store, &mount_path, device).map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn relink_device(state: State<'_, Shared>, device_id: String, mount_path: String) -> CmdResult<Device> {
    blocking(&state, move |s| {
        crate::devices::relink_device(&s.core.store, &device_id, &mount_path).map_err(|e| e.to_string())
    })
    .await
}
