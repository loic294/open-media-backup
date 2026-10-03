use super::{blocking, CmdResult, Shared};
use crate::app::{AppSettings, FilePage, ListFilesRequest, Snapshot};
use crate::plan::ProjectStatus;
use serde_json::Value;
use std::path::PathBuf;
use tauri::State;

#[tauri::command]
pub async fn get_snapshot(state: State<'_, Shared>) -> CmdResult<Snapshot> {
    blocking(&state, |s| s.core.snapshot()).await
}

#[tauri::command]
pub async fn save_entity(state: State<'_, Shared>, kind: String, entity: Value) -> CmdResult<()> {
    blocking(&state, move |s| s.core.save_entity(&kind, entity)).await
}

#[tauri::command]
pub async fn delete_entity(state: State<'_, Shared>, kind: String, id: String) -> CmdResult<()> {
    blocking(&state, move |s| s.core.delete_entity(&kind, &id)).await
}

#[tauri::command]
pub async fn save_settings(state: State<'_, Shared>, settings: AppSettings) -> CmdResult<()> {
    blocking(&state, move |s| s.core.save_settings(&settings)).await
}

#[tauri::command]
pub async fn get_project_status(state: State<'_, Shared>, project_id: String) -> CmdResult<ProjectStatus> {
    blocking(&state, move |s| s.core.project_status(&project_id)).await
}

#[tauri::command]
pub async fn list_files(state: State<'_, Shared>, req: ListFilesRequest) -> CmdResult<FilePage> {
    blocking(&state, move |s| s.core.list_files(&req)).await
}

#[tauri::command]
pub async fn thumbnail(state: State<'_, Shared>, abs_path: String) -> CmdResult<Option<PathBuf>> {
    blocking(&state, move |s| s.core.thumbnail(&PathBuf::from(abs_path))).await
}
