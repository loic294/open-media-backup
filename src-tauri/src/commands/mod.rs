//! Thin Tauri command wrappers over `AppCore` and `SyncService`.
pub(crate) mod catalog;
pub(crate) mod devices;
mod setup;
#[cfg(test)]
mod ipc_tests;
pub(crate) mod sync;
pub(crate) mod transfers;

use crate::app::AppCore;
use crate::sync::SyncService;
use std::sync::Arc;

pub use setup::setup;
#[cfg(test)]
use setup::init;

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
    tauri::async_runtime::spawn_blocking(move || work(&state)).await.map_err(|e| e.to_string())?
}

#[macro_export]
macro_rules! omb_handlers {
    () => {
        tauri::generate_handler![
            $crate::commands::catalog::get_snapshot,
            $crate::commands::catalog::save_entity,
            $crate::commands::catalog::delete_entity,
            $crate::commands::catalog::save_settings,
            $crate::commands::catalog::get_project_status,
            $crate::commands::catalog::list_files,
            $crate::commands::catalog::thumbnail,
            $crate::commands::transfers::run_flow,
            $crate::commands::transfers::run_all,
            $crate::commands::transfers::set_transfer_paused,
            $crate::commands::transfers::set_all_paused,
            $crate::commands::transfers::cancel_transfer,
            $crate::commands::transfers::list_transfers,
            $crate::commands::transfers::plan_wipe,
            $crate::commands::transfers::wipe,
            $crate::commands::devices::list_volumes,
            $crate::commands::devices::register_device,
            $crate::commands::devices::relink_device,
            $crate::commands::sync::sync_status,
            $crate::commands::sync::add_peer,
            $crate::commands::sync::remove_peer,
            $crate::commands::sync::sync_now,
        ]
    };
}
