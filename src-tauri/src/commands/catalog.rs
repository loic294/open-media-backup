use super::{blocking, CmdResult, Shared};
use crate::app::{
    AppImportFile, AppSettings, FilePage, ListFilesRequest, ListWorkspaceFilesRequest,
    PreparedAppImport, RevealKind, Snapshot,
};
use crate::metadata::MediaMetadata;
use crate::plan::{ProjectStatus, WorkspaceContext, WorkspaceStatus};
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
pub async fn validate_app_path(app_path: String) -> CmdResult<String> {
    blocking_app_path(PathBuf::from(app_path)).await
}

async fn blocking_app_path(path: PathBuf) -> CmdResult<String> {
    tauri::async_runtime::spawn_blocking(move || {
        crate::app::app_paths::validate_app_path(&path)
            .map(|path| crate::app::app_paths::path_to_string(&path))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn get_project_status(
    state: State<'_, Shared>,
    project_id: String,
) -> CmdResult<ProjectStatus> {
    blocking(&state, move |s| s.core.project_status(&project_id)).await
}

#[tauri::command]
pub async fn get_workspace_status(
    state: State<'_, Shared>,
    context: WorkspaceContext,
) -> CmdResult<WorkspaceStatus> {
    blocking(&state, move |s| s.core.workspace_status(&context)).await
}

#[tauri::command]
pub async fn get_source_safe_copy_details(
    state: State<'_, Shared>,
    context: WorkspaceContext,
    source_id: String,
) -> CmdResult<crate::app::SourceSafeCopyDetails> {
    blocking(&state, move |s| {
        s.core.source_safe_copy_details(&context, &source_id)
    })
    .await
}

#[tauri::command]
pub async fn save_device_safe_copy_rules(
    state: State<'_, Shared>,
    context: WorkspaceContext,
    device_id: String,
    rules: Vec<crate::domain::FileRule>,
) -> CmdResult<()> {
    blocking(&state, move |s| {
        s.core
            .save_device_safe_copy_rules(&context, &device_id, rules)
    })
    .await
}

#[tauri::command]
pub async fn list_files(state: State<'_, Shared>, req: ListFilesRequest) -> CmdResult<FilePage> {
    blocking(&state, move |s| s.core.list_files(&req)).await
}

#[tauri::command]
pub async fn list_workspace_files(
    state: State<'_, Shared>,
    req: ListWorkspaceFilesRequest,
) -> CmdResult<FilePage> {
    blocking(&state, move |s| s.core.list_workspace_files(&req)).await
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
        }
        .as_deref()
        .map(std::path::Path::new)
        .map(crate::app::app_paths::validate_app_path)
        .transpose()?
        .map(|path| crate::app::app_paths::path_to_string(&path));
        Ok((path, app))
    })
    .await?;
    tauri::async_runtime::spawn_blocking(move || open_paths_with_app(&[path], app.as_deref()))
        .await
        .map_err(|e| e.to_string())?
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct OpenAppImportResult {
    pub token: String,
    pub app_name: String,
    pub files: Vec<AppImportFile>,
}

#[tauri::command]
pub async fn open_flow_in_app(
    state: State<'_, Shared>,
    project_id: String,
    flow_id: String,
) -> CmdResult<OpenAppImportResult> {
    let prepared = blocking(&state, move |s| {
        s.core.prepare_app_import(&project_id, &flow_id)
    })
    .await?;
    launch_app_import(prepared).await
}

#[tauri::command]
pub async fn open_workspace_flow_in_app(
    state: State<'_, Shared>,
    context: WorkspaceContext,
    flow_id: String,
) -> CmdResult<OpenAppImportResult> {
    let prepared = blocking(&state, move |s| {
        s.core.prepare_workspace_app_import(&context, &flow_id)
    })
    .await?;
    launch_app_import(prepared).await
}

async fn launch_app_import(prepared: PreparedAppImport) -> CmdResult<OpenAppImportResult> {
    let token = prepared.token;
    let app_name = prepared.app_name;
    let app_path = prepared.app_path;
    let paths = prepared.paths;
    let files = prepared.files;
    tauri::async_runtime::spawn_blocking(move || {
        open_paths_with_app(&paths, Some(app_path.as_str()))
    })
    .await
    .map_err(|e| e.to_string())??;
    Ok(OpenAppImportResult {
        token,
        app_name,
        files,
    })
}

#[tauri::command]
pub async fn reveal_in_file_manager(
    state: State<'_, Shared>,
    project_id: Option<String>,
    kind: RevealKind,
    id: String,
) -> CmdResult<()> {
    let path = blocking(&state, move |s| {
        s.core
            .reveal_file_manager_path(project_id.as_deref(), kind, &id)
    })
    .await?;
    tauri::async_runtime::spawn_blocking(move || reveal_path(&path))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn confirm_app_import(
    state: State<'_, Shared>,
    project_id: String,
    flow_id: String,
    token: String,
) -> CmdResult<usize> {
    blocking(&state, move |s| {
        s.core.confirm_app_import(&project_id, &flow_id, &token)
    })
    .await
}

#[tauri::command]
pub async fn confirm_workspace_app_import(
    state: State<'_, Shared>,
    context: WorkspaceContext,
    flow_id: String,
    token: String,
) -> CmdResult<usize> {
    blocking(&state, move |s| {
        s.core
            .confirm_workspace_app_import(&context, &flow_id, &token)
    })
    .await
}

fn open_paths_with_app(paths: &[PathBuf], app: Option<&str>) -> CmdResult<()> {
    let app = app.map(str::trim).filter(|app| !app.is_empty());
    if paths.is_empty() {
        return Ok(());
    }
    for chunk in chunk_paths(paths) {
        let mut command = open_command(app, chunk);
        command
            .spawn()
            .map(|_| ())
            .map_err(|error| format!("Could not launch the media app: {error}"))?;
    }
    Ok(())
}

fn chunk_paths(paths: &[PathBuf]) -> Vec<&[PathBuf]> {
    const MAX_ARGS: usize = 400;
    const MAX_BYTES: usize = 96 * 1024;
    let mut chunks = Vec::new();
    let mut start = 0;
    let mut bytes = 0;
    for (i, path) in paths.iter().enumerate() {
        let len = path.as_os_str().to_string_lossy().len().max(1);
        if i > start && (i - start >= MAX_ARGS || bytes + len > MAX_BYTES) {
            chunks.push(&paths[start..i]);
            start = i;
            bytes = 0;
        }
        bytes += len;
    }
    chunks.push(&paths[start..]);
    chunks
}

fn open_command(app: Option<&str>, paths: &[PathBuf]) -> Command {
    #[cfg(target_os = "macos")]
    {
        let mut command = Command::new("open");
        if let Some(app) = app {
            command.arg("-a").arg(app);
        }
        command.args(paths);
        command
    }
    #[cfg(target_os = "windows")]
    {
        let mut command = match app {
            Some(app) => Command::new(app),
            None => Command::new("explorer.exe"),
        };
        command.args(paths);
        command
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let mut command = match app {
            Some(app) => Command::new(app),
            None => Command::new("xdg-open"),
        };
        command.args(paths);
        command
    }
}

fn reveal_path(path: &std::path::Path) -> CmdResult<()> {
    let mut command = reveal_command(path);
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Could not open the folder in the file manager: {error}"))
}

fn reveal_command(path: &std::path::Path) -> Command {
    #[cfg(target_os = "macos")]
    {
        let mut command = Command::new("open");
        if path.is_file() {
            command.arg("-R");
        }
        command.arg(path);
        command
    }
    #[cfg(target_os = "windows")]
    {
        let mut command = Command::new("explorer.exe");
        if path.is_file() {
            command.arg("/select,").arg(path);
        } else {
            command.arg(path);
        }
        command
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let mut command = Command::new("xdg-open");
        if path.is_file() {
            command.arg(path.parent().unwrap_or(path));
        } else {
            command.arg(path);
        }
        command
    }
}
