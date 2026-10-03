use std::fs::Metadata;
use std::path::Path;
use std::time::UNIX_EPOCH;

use xxhash_rust::xxh64::Xxh64;

use crate::thumbnails::ThumbnailError;

pub fn cache_key(path: &Path, metadata: &Metadata) -> Result<String, ThumbnailError> {
    let absolute = path.canonicalize().map_err(|source| ThumbnailError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let modified = metadata.modified().map_err(|source| ThumbnailError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let modified_nanos = modified
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();

    let mut hasher = Xxh64::new(0);
    hasher.update(absolute.to_string_lossy().as_bytes());
    hasher.update(&[0]);
    hasher.update(&metadata.len().to_le_bytes());
    hasher.update(&[0]);
    hasher.update(&modified_nanos.to_le_bytes());
    Ok(format!("{:016x}", hasher.digest()))
}
