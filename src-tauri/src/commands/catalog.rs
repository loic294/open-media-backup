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

/// Opens only a source-listed media path with the selected application or the OS default.
#[tauri::command]
pub async fn open_media_file(state: State<'_, Shared>, abs_path: String) -> CmdResult<()> {
    // The app comes from this device's stored settings, never from the webview.
    let (path, app) = blocking(&state, move |s| {
        let path = s.core.authorize_media_open(&PathBuf::from(abs_path))?;
        let apps = s.core.settings().preview_apps;
        let app = match crate::media::media_kind(&path) {
            crate::media::MediaKind::Video => apps.videos,
            _ => apps.photos,
        };
        Ok((path, app))
    })
    .await?;
    tauri::async_runtime::spawn_blocking(move || open_with_app(&path, app.as_deref()))
        .await
        .map_err(|e| e.to_string())?
}

fn open_with_app(path: &std::path::Path, app: Option<&str>) -> CmdResult<()> {
    let app = app.map(str::trim).filter(|app| !app.is_empty());
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("open");
        if let Some(app) = app {
            command.arg("-a").arg(app);
        }
        command.arg(path);
        command
    };
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = match app {
            Some(app) => Command::new(app),
            None => Command::new("explorer.exe"),
        };
        command.arg(path);
        command
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = {
        let mut command = match app {
            Some(app) => Command::new(app),
            None => Command::new("xdg-open"),
        };
        command.arg(path);
        command
    };
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Could not launch the media app: {error}"))
}
