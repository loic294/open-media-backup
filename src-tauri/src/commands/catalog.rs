use super::{blocking, CmdResult, Shared};
use crate::app::{AppSettings, FilePage, ListFilesRequest, Snapshot};
use crate::metadata::MediaMetadata;
use crate::plan::ProjectStatus;
use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;
use tauri::ipc::Response;
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
pub async fn get_project_status(
    state: State<'_, Shared>,
    project_id: String,
) -> CmdResult<ProjectStatus> {
    blocking(&state, move |s| s.core.project_status(&project_id)).await
}

#[tauri::command]
pub async fn list_files(state: State<'_, Shared>, req: ListFilesRequest) -> CmdResult<FilePage> {
    blocking(&state, move |s| s.core.list_files(&req)).await
}

#[tauri::command]
pub async fn get_media_metadata(abs_path: String) -> CmdResult<MediaMetadata> {
    blocking_path(PathBuf::from(abs_path)).await
}

async fn blocking_path(path: PathBuf) -> CmdResult<MediaMetadata> {
    tauri::async_runtime::spawn_blocking(move || {
        crate::metadata::extract_metadata(&path).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// JPEG bytes of a cached thumbnail, sent as a raw IPC payload (empty when none can be made).
/// Avoids the asset protocol, so no filesystem scope or CSP exception is needed.
#[tauri::command]
pub async fn thumbnail(state: State<'_, Shared>, abs_path: String) -> CmdResult<Response> {
    blocking(&state, move |s| {
        let bytes = match s.core.thumbnail(&PathBuf::from(abs_path))? {
            Some(path) => std::fs::read(path).map_err(|e| e.to_string())?,
            None => Vec::new(),
        };
        Ok(Response::new(bytes))
    })
    .await
}

/// JPEG still preview sized for the in-app viewer, sent as raw IPC bytes.
#[tauri::command]
pub async fn media_preview(state: State<'_, Shared>, abs_path: String) -> CmdResult<Response> {
    blocking(&state, move |s| {
        let bytes = match s.core.media_preview(&PathBuf::from(abs_path))? {
            Some(path) => std::fs::read(path).map_err(|e| e.to_string())?,
            None => Vec::new(),
        };
        Ok(Response::new(bytes))
    })
    .await
}

/// Grants the webview access to a listed video file so its native video element can stream it.
#[tauri::command]
pub async fn authorize_video_preview(state: State<'_, Shared>, abs_path: String) -> CmdResult<()> {
    blocking(&state, move |s| {
        let path = s.core.authorize_video_preview(&PathBuf::from(abs_path))?;
        (s.video_preview_scope)(path)
    })
    .await
}

/// Opens only a source-listed media path with the operating system's default application.
#[tauri::command]
pub async fn open_media(state: State<'_, Shared>, abs_path: String) -> CmdResult<()> {
    let path = blocking(&state, move |s| {
        s.core.authorize_media_open(&PathBuf::from(abs_path))
    })
    .await?;
    tauri::async_runtime::spawn_blocking(move || open_with_default_app(&path))
        .await
        .map_err(|e| e.to_string())?
}

fn open_with_default_app(path: &std::path::Path) -> CmdResult<()> {
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("open");
        command.arg(path);
        command
    };
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = Command::new("explorer.exe");
        command.arg(path);
        command
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = {
        let mut command = Command::new("xdg-open");
        command.arg(path);
        command
    };
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Could not launch the default media app: {error}"))
}
