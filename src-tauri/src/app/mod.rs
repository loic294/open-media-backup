//! Application services shared by the Tauri commands. Free of Tauri types so it can be tested.
pub(crate) mod app_paths;
mod core;
mod entities;
mod files;
mod jobs;
mod resolver;
mod reveal;
mod settings;
mod snapshot;

pub use core::{AppCore, AppImportFile, PreparedAppImport};
pub use files::{FileEntry, FilePage, ListFilesRequest, ListWorkspaceFilesRequest};
pub use resolver::DeviceResolver;
pub use reveal::{resolve_reveal_path, reveal_space_id, RevealKind};
pub use settings::AppSettings;
pub use snapshot::Snapshot;

#[cfg(test)]
mod tests;
