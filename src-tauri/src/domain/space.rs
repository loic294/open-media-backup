use super::entity::impl_entity;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HashAlgo {
    #[default]
    Xxh64,
    Blake3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerifyMode {
    /// Hash the bytes while copying them (fast, does not detect write corruption).
    Inline,
    /// Copy, then re-read the destination file and compare hashes.
    #[default]
    Reread,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct VariableDef {
    pub name: String,
    pub default_value: String,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Space {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub position: i64,
    pub hash_algo: HashAlgo,
    pub verify_mode: VerifyMode,
    pub variables: Vec<VariableDef>,
    /// Template used to create a backup-folder marker on an original device when missing.
    pub backup_marker_template: String,
}
impl_entity!(Space, Space);
