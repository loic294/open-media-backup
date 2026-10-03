use super::{entity::impl_entity, FileRule};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DestinationKind {
    #[default]
    Folder,
    App,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Destination {
    pub id: String,
    pub space_id: String,
    pub kind: DestinationKind,
    pub device_id: String,
    pub path_template: String,
    pub app_name: Option<String>,
    pub subfolder_per_source: bool,
    pub counts_as_safe_copy: bool,
    /// Use the backup-folder marker stored on the original device instead of project variables.
    pub use_backup_marker: bool,
    pub rules: Vec<FileRule>,
    pub position: i64,
}
impl_entity!(Destination, Destination);

impl Default for Destination {
    fn default() -> Self {
        Self {
            id: String::new(),
            space_id: String::new(),
            kind: DestinationKind::Folder,
            device_id: String::new(),
            path_template: String::new(),
            app_name: None,
            subfolder_per_source: true,
            counts_as_safe_copy: true,
            use_backup_marker: false,
            rules: Vec::new(),
            position: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Flow {
    pub id: String,
    pub space_id: String,
    pub source_id: String,
    pub destination_id: String,
}
impl_entity!(Flow, Flow);
