use super::{entity::impl_entity, HashAlgo};
use serde::{Deserialize, Serialize};

/// A unique piece of media identified by its content hash. Its id is `{algo}:{hash}`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct FileRecord {
    pub id: String,
    pub hash: String,
    pub hash_algo: HashAlgo,
    pub size: u64,
    pub name: String,
    pub origin_device_id: String,
    pub origin_path: String,
    pub modified_at: Option<i64>,
}
impl_entity!(FileRecord, FileRecord);

impl FileRecord {
    pub fn id_for(algo: HashAlgo, hash: &str) -> String {
        let prefix = match algo {
            HashAlgo::Xxh64 => "xxh64",
            HashAlgo::Blake3 => "blake3",
        };
        format!("{prefix}:{hash}")
    }
}

/// A verified copy of a file on a device. Its id is `{file_id}@{device_id}:{path}`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct FileCopy {
    pub id: String,
    pub file_id: String,
    pub device_id: String,
    /// Path relative to the device root, always using `/`.
    pub path: String,
    pub verified_at: i64,
    pub removed: bool,
}
impl_entity!(FileCopy, FileCopy);

impl FileCopy {
    pub fn id_for(file_id: &str, device_id: &str, path: &str) -> String {
        format!("{file_id}@{device_id}:{path}")
    }
}

/// An explicit user acknowledgement that a skipped, different destination file is safe.
/// This is separate from `FileCopy`: it does not claim the destination has the source hash.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SafeCopyOverride {
    pub id: String,
    pub space_id: String,
    pub file_id: String,
    pub source_device_id: String,
    pub source_path: String,
    pub destination_device_id: String,
    pub destination_path: String,
    pub destination_hash: String,
}
impl_entity!(SafeCopyOverride, SafeCopyOverride);

impl SafeCopyOverride {
    pub fn id_for(
        space_id: &str,
        file_id: &str,
        destination_device_id: &str,
        destination_path: &str,
    ) -> String {
        format!("{space_id}:{file_id}@{destination_device_id}:{destination_path}")
    }
}
