use crate::HashAlgo;
use serde::{Deserialize, Serialize};
use std::{
    io,
    path::{Component, Path},
};

pub const DEFAULT_PORT: u16 = 47822;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hello {
    pub server_id: String,
    pub name: String,
    pub version: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Root {
    pub id: String,
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Browse {
    pub path: String,
    pub directories: Vec<String>,
}

/// One non-recursive directory listing. Symlinks and special files are reported
/// in `other` so clients can surface them instead of treating them as missing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Listing {
    pub path: String,
    pub directories: Vec<String>,
    pub files: Vec<ListedFile>,
    #[serde(default)]
    pub other: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListedFile {
    pub name: String,
    pub size: u64,
    pub modified: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HashRequest {
    pub root: String,
    pub rel_path: String,
    pub algo: HashAlgo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HashResponse {
    pub hash: String,
    pub size: u64,
    pub modified: Option<u64>,
}

pub fn validate_relative(path: &str) -> io::Result<()> {
    if path.contains(['\\', '\0', ':'])
        || path.split('/').any(|p| p == "..")
        || Path::new(path).components().any(|c| {
            matches!(
                c,
                Component::RootDir | Component::ParentDir | Component::Prefix(_)
            )
        })
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "unsafe relative path",
        ));
    }
    Ok(())
}
