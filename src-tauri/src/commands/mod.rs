//! Thin Tauri command wrappers over `AppCore` and `SyncService`.
pub(crate) mod catalog;
pub(crate) mod devices;
pub(crate) mod hash_server;
#[cfg(test)]
mod ipc_tests;
pub(crate) mod menu;
mod setup;
pub(crate) mod sync;
pub(crate) mod transfers;
pub(crate) mod updater;

use crate::app::AppCore;
use crate::sync::SyncService;
use std::sync::Arc;

#[cfg(test)]
use setup::init;
pub use setup::setup;

pub struct AppState {
    pub core: AppCore,
    pub sync: SyncService,
}

pub type Shared = Arc<AppState>;
pub type CmdResult<T> = Result<T, String>;

/// Runs blocking work (disk scans, hashing, SQLite) off the main thread.
pub(crate) async fn blocking<T: Send + 'static>(
    state: &tauri::State<'_, Shared>,
    work: impl FnOnce(&AppState) -> CmdResult<T> + Send + 'static,
) -> CmdResult<T> {
    let state = Arc::clone(state);
    tauri::async_runtime::spawn_blocking(move || work(&state))
        .await
        .map_err(|e| e.to_string())?
}

#[macro_export]
macro_rules! omb_handlers {
    () => {
        tauri::generate_handler![
            $crate::commands::catalog::get_snapshot,
            $crate::commands::catalog::save_entity,
            $crate::commands::catalog::delete_entity,
            $crate::commands::catalog::save_settings,
            $crate::commands::catalog::validate_app_path,
            $crate::commands::catalog::get_project_status,
            $crate::commands::catalog::get_workspace_status,
            $crate::commands::catalog::get_source_safe_copy_details,
            $crate::commands::catalog::save_device_safe_copy_rules,
            $crate::commands::catalog::list_files,
            $crate::commands::catalog::list_workspace_files,
            $crate::commands::catalog::get_media_metadata,
            $crate::commands::catalog::thumbnail,
            $crate::commands::catalog::open_media_file,
            $crate::commands::catalog::open_flow_in_app,
            $crate::commands::catalog::open_workspace_flow_in_app,
            $crate::commands::catalog::reveal_in_file_manager,
            $crate::commands::catalog::confirm_app_import,
            $crate::commands::catalog::confirm_workspace_app_import,
            $crate::commands::updater::check_for_update,
            $crate::commands::updater::install_update,
            $crate::commands::transfers::run_flow,
            $crate::commands::transfers::run_all,
            $crate::commands::transfers::run_workspace_flow,
            $crate::commands::transfers::run_workspace_all,
            $crate::commands::transfers::check_workspace_destination,
            $crate::commands::transfers::run_workspace_destination,
            $crate::commands::transfers::resolve_transfer_conflict,
            $crate::commands::transfers::set_transfer_paused,
            $crate::commands::transfers::set_all_paused,
            $crate::commands::transfers::cancel_transfer,
            $crate::commands::transfers::list_transfers,
            $crate::commands::transfers::get_speed_analysis,
            $crate::commands::transfers::list_speed_analysis_jobs,
            $crate::commands::transfers::plan_wipe,
            $crate::commands::transfers::mark_source_manually_wiped,
            $crate::commands::transfers::wipe,
            $crate::commands::devices::list_volumes,
            $crate::commands::devices::connect_destination_network_drive,
            $crate::commands::devices::register_device,
            $crate::commands::devices::relink_device,
            $crate::commands::sync::sync_status,
            $crate::commands::sync::add_peer,
            $crate::commands::sync::remove_peer,
            $crate::commands::sync::sync_now,
            $crate::commands::hash_server::list_hash_servers,
            $crate::commands::hash_server::add_hash_server,
            $crate::commands::hash_server::remove_hash_server,
            $crate::commands::hash_server::hash_server_roots,
            $crate::commands::hash_server::hash_server_browse,
            $crate::commands::hash_server::test_remote_hash_mapping,
            $crate::commands::menu::set_spaces_menu,
        ]
    };
}
