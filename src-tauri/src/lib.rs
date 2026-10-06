pub mod app;
mod commands;
pub mod devices;
pub mod domain;
pub mod hash_server;
pub mod hashing;
pub mod media;
pub mod metadata;
pub mod paths;
pub mod plan;
pub mod rules;
pub mod scan;
pub mod store;
pub mod sync;
#[cfg(test)]
pub mod testing;
pub mod thumbnails;
pub mod transfer;
pub mod wipe;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .setup(commands::setup)
        .invoke_handler(omb_handlers!())
        .run(context())
        .expect("error while running Open Media Backup");
}

/// Embedded config, assets and capabilities (shared with IPC tests).
pub(crate) fn context<R: tauri::Runtime>() -> tauri::Context<R> {
    tauri::generate_context!()
}
