//! Walks a source folder and lists candidate media files.
use crate::paths::to_relative;
use std::path::{Path, PathBuf};
use walkdir::{DirEntry, WalkDir};

#[derive(Debug, Clone, PartialEq)]
pub struct ScannedFile {
    pub abs_path: PathBuf,
    /// Relative to the scanned folder, `/`-separated.
    pub rel_path: String,
    pub size: u64,
    pub modified_ms: Option<i64>,
}

const SKIPPED_DIRS: &[&str] = &[
    ".openmediabackup",
    ".Spotlight-V100",
    ".Trashes",
    ".fseventsd",
    ".TemporaryItems",
    ".DocumentRevisions-V100",
    "System Volume Information",
    "$RECYCLE.BIN",
    "@eaDir",
    "#recycle",
];
const SKIPPED_FILES: &[&str] = &[".DS_Store", "Thumbs.db", "desktop.ini"];

fn is_skipped(entry: &DirEntry) -> bool {
    let name = entry.file_name().to_string_lossy();
    if entry.depth() == 0 {
        return false;
    }
    if entry.file_type().is_dir() {
        SKIPPED_DIRS.contains(&name.as_ref())
    } else {
        SKIPPED_FILES.contains(&name.as_ref())
            || name.starts_with("._")
            || name.ends_with(".omb-partial")
    }
}

/// Lists files below `folder`, sorted by relative path. Missing folder → empty list.
pub fn scan_folder(folder: &Path) -> Vec<ScannedFile> {
    let mut files: Vec<ScannedFile> = WalkDir::new(folder)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| !is_skipped(e))
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            Some(ScannedFile {
                rel_path: to_relative(folder, e.path())?,
                abs_path: e.into_path(),
                size: meta.len(),
                modified_ms: meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as i64),
            })
        })
        .collect();
    files.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    files
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn scans_and_skips_system_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("DCIM/100")).unwrap();
        fs::create_dir_all(root.join(".Trashes/x")).unwrap();
        fs::create_dir_all(root.join(".openmediabackup")).unwrap();
        fs::write(root.join("DCIM/100/A.ARW"), b"12345").unwrap();
        fs::write(root.join("DCIM/100/._A.ARW"), b"x").unwrap();
        fs::write(root.join(".DS_Store"), b"x").unwrap();
        fs::write(root.join(".Trashes/x/b"), b"x").unwrap();
        fs::write(root.join(".openmediabackup/device.json"), b"{}").unwrap();
        let files = scan_folder(root);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].rel_path, "DCIM/100/A.ARW");
        assert_eq!(files[0].size, 5);
        assert!(scan_folder(&root.join("missing")).is_empty());
    }
}
