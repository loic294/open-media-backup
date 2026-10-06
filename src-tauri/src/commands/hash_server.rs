use super::{blocking, CmdResult, Shared};
use crate::{
    hash_server::{service, service::MappingTest, Browse, Root},
    store::HashServer,
};
use tauri::State;

#[tauri::command]
pub fn list_hash_servers(state: State<'_, Shared>) -> CmdResult<Vec<HashServer>> {
    state.core.store.hash_servers().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn add_hash_server(
    state: State<'_, Shared>,
    address: String,
    token: String,
) -> CmdResult<()> {
    service::add(&state.core.store, &address, &token).await
}

#[tauri::command]
pub fn remove_hash_server(state: State<'_, Shared>, id: String) -> CmdResult<()> {
    state
        .core
        .store
        .remove_hash_server(&id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn hash_server_roots(state: State<'_, Shared>, id: String) -> CmdResult<Vec<Root>> {
    service::roots(&state.core.store, &id).await
}

#[tauri::command]
pub async fn hash_server_browse(
    state: State<'_, Shared>,
    id: String,
    root: String,
    path: String,
) -> CmdResult<Browse> {
    service::browse(&state.core.store, &id, &root, &path).await
}

#[tauri::command]
pub async fn test_remote_hash_mapping(
    state: State<'_, Shared>,
    destination_id: String,
) -> CmdResult<MappingTest> {
    blocking(&state, move |state| {
        service::test_mapping(
            &state.core.store,
            state.core.resolver.as_ref(),
            &destination_id,
        )
    })
    .await
}
