use super::{CmdResult, Shared};
use crate::sync::SyncStatus;
use tauri::State;

#[tauri::command]
pub fn sync_status(state: State<'_, Shared>) -> SyncStatus {
    state.sync.status()
}

#[tauri::command]
pub async fn add_peer(state: State<'_, Shared>, address: String, token: String) -> CmdResult<()> {
    let sync = state.sync.clone();
    sync.add_peer(&address, &token)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn remove_peer(state: State<'_, Shared>, peer_id: String) -> CmdResult<()> {
    state.sync.remove_peer(&peer_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn sync_now(state: State<'_, Shared>) -> CmdResult<()> {
    let sync = state.sync.clone();
    sync.sync_now().await.map_err(|e| e.to_string())
}
