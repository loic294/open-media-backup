use serde::{Deserialize, Serialize};
use std::{fs, io, path::Path};

/// Stored on an original device; names the folder its content is backed up into.
pub const BACKUP_MARKER_FILE: &str = ".openmediabackup/backup-folder.json";

#[derive(Debug, Serialize, Deserialize)]
struct BackupMarker {
    folder: String,
    created_at: i64,
}

pub fn read_backup_folder(device_root: &Path) -> Option<String> {
    let text = fs::read_to_string(device_root.join(BACKUP_MARKER_FILE)).ok()?;
    let marker: BackupMarker = serde_json::from_str(&text).ok()?;
    Some(marker.folder).filter(|f| !f.trim().is_empty())
}

/// Returns the existing folder name or writes `default_folder` as the new marker.
pub fn ensure_backup_folder(device_root: &Path, default_folder: &str) -> io::Result<String> {
    if let Some(existing) = read_backup_folder(device_root) {
        return Ok(existing);
    }
    let path = device_root.join(BACKUP_MARKER_FILE);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let marker = BackupMarker {
        folder: default_folder.to_string(),
        created_at: chrono::Utc::now().timestamp_millis(),
    };
    fs::write(&path, serde_json::to_vec_pretty(&marker)?)?;
    Ok(marker.folder)
}

pub fn remove_backup_folder(device_root: &Path) -> io::Result<()> {
    match fs::remove_file(device_root.join(BACKUP_MARKER_FILE)) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_creates_once_then_reuses() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read_backup_folder(dir.path()), None);
        assert_eq!(
            ensure_backup_folder(dir.path(), "2026-03-01_Iceland").unwrap(),
            "2026-03-01_Iceland"
        );
        assert_eq!(
            ensure_backup_folder(dir.path(), "other").unwrap(),
            "2026-03-01_Iceland"
        );
        remove_backup_folder(dir.path()).unwrap();
        assert_eq!(read_backup_folder(dir.path()), None);
        remove_backup_folder(dir.path()).unwrap();
    }
}
