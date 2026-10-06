use super::entity::impl_entity;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HashAlgo {
    Xxh64,
    #[default]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    pub allow_project_overlap: bool,
    /// Number of verified temporary destination copies that equal one final copy.
    /// Zero preserves the default behavior: temporary destinations never count.
    pub temporary_copies_per_final: u32,
    /// Deliberate transfer conflict skips count as an explicit safe-copy acknowledgement.
    pub skip_counts_as_safe_copy: bool,
}
impl_entity!(Space, Space);

impl Default for Space {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            icon: String::new(),
            position: 0,
            hash_algo: HashAlgo::default(),
            verify_mode: VerifyMode::default(),
            variables: Vec::new(),
            backup_marker_template: String::new(),
            allow_project_overlap: true,
            temporary_copies_per_final: 0,
            skip_counts_as_safe_copy: false,
        }
    }
}
