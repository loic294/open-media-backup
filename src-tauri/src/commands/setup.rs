use super::{AppState, Shared};
use crate::app::{AppCore, AppSettings, DeviceResolver};
use crate::store::Store;
use crate::sync::SyncService;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tauri::{App, AppHandle, Emitter, Manager, Runtime};

const CATALOG_KINDS: [&str; 2] = ["file_record", "file_copy"];

/// Opens the database, starts background services and wires their events to the UI.
pub fn setup(app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = app.path().app_data_dir()?;
    init(app.handle(), &data_dir)
}

/// Same as `setup` with an explicit data directory (used by IPC tests).
pub fn init<R: Runtime>(
    handle: &AppHandle<R>,
    data_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(data_dir)?;
    let store = Arc::new(Store::open(&data_dir.join("omb.sqlite3"))?);
    let handle = handle.clone();

    let emitter = handle.clone();
    let core = AppCore::new(
        store.clone(),
        Arc::new(DeviceResolver(store.clone())),
        data_dir.join("thumbnails"),
        move |jobs| {
            let _ = emitter.emit("transfers", jobs);
        },
    );
    core.register_computer()?;

    let emitter = handle.clone();
    let sync = SyncService::new(store.clone(), move |status| {
        let _ = emitter.emit("sync-status", status);
    });
    handle.manage::<Shared>(Arc::new(AppState {
        core,
        sync: sync.clone(),
    }));

    start_sync(sync, store.clone());
    forward_store_changes(handle.clone(), store.clone());
    let emitter = handle;
    crate::devices::spawn_watcher(store, move |volumes| {
        let _ = emitter.emit("volumes-changed", volumes);
        let _ = emitter.emit("status-changed", ());
    });
    Ok(())
}

fn start_sync(sync: SyncService, store: Arc<Store>) {
    tauri::async_runtime::spawn(async move {
        let port = AppSettings::load(&store).sync_port;
        if let Err(e) = sync.start_server(port).await {
            log::error!("sync server could not listen on port {port}: {e}");
        }
        let interval_store = store.clone();
        sync.spawn_auto_sync(Arc::new(move || {
            let settings = AppSettings::load(&interval_store);
            settings
                .auto_sync
                .then(|| Duration::from_secs(u64::from(settings.auto_sync_minutes.max(1)) * 60))
        }));
    });
}

/// Coalesces store changes into `snapshot-changed` (configuration) and `status-changed` events.
fn forward_store_changes<R: Runtime>(handle: AppHandle<R>, store: Arc<Store>) {
    let mut changes = store.subscribe();
    tauri::async_runtime::spawn(async move {
        while let Ok(first) = changes.recv().await {
            tokio::time::sleep(Duration::from_millis(150)).await;
            let mut kinds = first.kinds;
            while let Ok(more) = changes.try_recv() {
                kinds.extend(more.kinds);
            }
            if kinds.iter().any(|k| !CATALOG_KINDS.contains(&k.as_str())) {
                let _ = handle.emit("snapshot-changed", ());
            }
            let _ = handle.emit("status-changed", ());
        }
    });
}
