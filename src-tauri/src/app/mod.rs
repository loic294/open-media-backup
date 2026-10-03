//! Application services shared by the Tauri commands. Free of Tauri types so it can be tested.
mod core;
mod entities;
mod files;
mod jobs;
mod resolver;
mod settings;
mod snapshot;

pub use core::{AppCore, AppImportFile, PreparedAppImport};
pub use files::{FileEntry, FilePage, ListFilesRequest};
pub use resolver::DeviceResolver;
pub use settings::AppSettings;
pub use snapshot::Snapshot;

#[cfg(test)]
mod tests;
