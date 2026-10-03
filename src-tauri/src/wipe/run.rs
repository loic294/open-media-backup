use super::format::quick_format;
use super::plan::{assess, Assessed};
use crate::domain::FileCopy;
use crate::hashing::hash_file;
use crate::paths::{join_relative, remove_backup_folder};
use crate::plan::RootResolver;
use crate::store::Store;
use crate::transfer::{JobHandle, JobState};
use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WipeMethod {
    /// Deletes only files with enough verified copies, keeping folders and unknown files.
    DeleteFiles,
    /// Erases the whole volume (exFAT) and re-writes the device marker.
    QuickFormat,
}

/// Re-verifies every source file against its catalogued hash, then wipes the device.
/// Nothing is deleted if any file fails verification.
pub fn wipe(
    store: &Store,
    resolver: &dyn RootResolver,
    project_id: &str,
    source_id: &str,
    method: WipeMethod,
    handle: &JobHandle,
) -> Result<(), String> {
    let Assessed { device, catalog, assessment, source } = assess(store, resolver, project_id, source_id)?;
    if !assessment.status.wipe_eligible || !source.offer_wipe {
        return Err(assessment.status.blocking_reason.unwrap_or_else(|| "Wiping is disabled for this source".into()));
    }
    let root = assessment.root.clone().ok_or("device is not connected")?;
    let known: Vec<(String, String)> = assessment
        .known
        .iter()
        .filter_map(|(rel, id)| Some((assessment.device_path(rel), id.clone()?)))
        .collect();
    handle.update(|j| {
        j.label = format!("Wipe {}", device.name);
        j.files_total = known.len();
        j.bytes_total = assessment.files.iter().map(|f| f.size).sum();
        j.state = JobState::Verifying;
    });

    for (path, id) in &known {
        handle.checkpoint().map_err(|_| "cancelled".to_string())?;
        handle.update(|j| j.current_file = Some(path.clone()));
        let record = catalog.record(id).ok_or_else(|| format!("{path}: missing catalog record"))?;
        let actual = hash_file(&join_relative(&root, path), record.hash_algo, |n| {
            handle.add_bytes(n);
            handle.checkpoint().is_ok()
        })
        .map_err(|e| format!("{path}: {e}"))?;
        if actual != record.hash {
            return Err(format!("{path} changed since it was backed up; nothing was wiped"));
        }
        handle.update(|j| j.files_done += 1);
    }
    handle.checkpoint().map_err(|_| "cancelled".to_string())?;
    handle.update(|j| {
        j.state = JobState::Running;
        j.current_file = None;
    });

    let removed: Vec<FileCopy> = match method {
        WipeMethod::DeleteFiles => {
            let mut removed = Vec::new();
            for (path, id) in &known {
                std::fs::remove_file(join_relative(&root, path)).map_err(|e| format!("{path}: {e}"))?;
                removed.push(mark_removed(id, &device.id, path));
            }
            let _ = remove_backup_folder(&root);
            removed
        }
        WipeMethod::QuickFormat => {
            quick_format(&root, &device.name)?;
            if let Err(e) = crate::devices::write_marker(&root, &device) {
                log::warn!("could not rewrite device marker after format: {e}");
            }
            catalog.copies_under(&device.id, "").map(|c| mark_removed(&c.file_id, &device.id, &c.path)).collect()
        }
    };
    store.put_all(&removed).map_err(|e| e.to_string())
}

fn mark_removed(file_id: &str, device_id: &str, path: &str) -> FileCopy {
    FileCopy {
        id: FileCopy::id_for(file_id, device_id, path),
        file_id: file_id.into(),
        device_id: device_id.into(),
        path: path.into(),
        verified_at: chrono::Utc::now().timestamp_millis(),
        removed: true,
    }
}
