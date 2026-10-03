//! Path templates (`{variable}` expansion), safe relative joins and the backup-folder marker.
mod backup_marker;
mod join;
mod template;

pub use backup_marker::{
    ensure_backup_folder, read_backup_folder, remove_backup_folder, BACKUP_MARKER_FILE,
};
pub use join::{join_relative, to_relative};
pub use template::{expand, TemplateError, TemplateVars};
