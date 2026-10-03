use super::{Catalog, FlowContext};
use crate::paths::join_relative;
use crate::scan::{scan_folder, ScannedFile};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    ToTransfer,
    Transferred,
    Ignored,
    Error,
}

#[derive(Debug, Clone)]
pub struct PlannedFile {
    /// Relative to the source folder.
    pub rel_path: String,
    pub abs_path: Option<PathBuf>,
    pub size: u64,
    pub modified_ms: Option<i64>,
    pub file_id: Option<String>,
    pub category: Category,
    pub error: Option<String>,
}

/// Classifies every file of a flow's source. Uses the catalog when the source is offline.
pub fn classify_flow(
    ctx: &FlowContext,
    catalog: &Catalog,
    failures: Option<&HashMap<String, String>>,
) -> Vec<PlannedFile> {
    let files = source_files(
        ctx.source_root.as_deref(),
        &ctx.source_folder_rel,
        &ctx.source_device.id,
        catalog,
    );
    classify_files(ctx, catalog, files, failures)
}

/// Files of a source folder: scanned when mounted, else the catalog's known copies.
pub fn source_files(
    root: Option<&Path>,
    folder_rel: &str,
    device_id: &str,
    catalog: &Catalog,
) -> Vec<ScannedFile> {
    match root {
        Some(root) => scan_folder(&join_relative(root, folder_rel)),
        None => offline_files(folder_rel, device_id, catalog),
    }
}

pub fn classify_files(
    ctx: &FlowContext,
    catalog: &Catalog,
    files: Vec<ScannedFile>,
    failures: Option<&HashMap<String, String>>,
) -> Vec<PlannedFile> {
    files
        .into_iter()
        .map(|file| {
            let device_path = ctx.source_device_path(&file.rel_path);
            let known = catalog.file_at(&ctx.source_device.id, &device_path, Some(file.size));
            let file_id = known.map(|r| r.id.clone());
            let error = failures.and_then(|f| f.get(&file.rel_path)).cloned();
            let category = if !ctx.rules.allows(&file.rel_path) {
                Category::Ignored
            } else if file_id
                .as_ref()
                .is_some_and(|id| catalog.has_copy_on(id, &ctx.dest_device.id))
            {
                Category::Transferred
            } else if error.is_some() {
                Category::Error
            } else {
                Category::ToTransfer
            };
            PlannedFile {
                rel_path: file.rel_path,
                abs_path: ctx.source_root.is_some().then_some(file.abs_path),
                size: file.size,
                modified_ms: file.modified_ms,
                file_id,
                category,
                error: if category == Category::Error {
                    error
                } else {
                    None
                },
            }
        })
        .collect()
}

fn offline_files(folder_rel: &str, device_id: &str, catalog: &Catalog) -> Vec<ScannedFile> {
    let prefix_len = if folder_rel.is_empty() {
        0
    } else {
        folder_rel.len() + 1
    };
    let mut files: Vec<ScannedFile> = catalog
        .copies_under(device_id, folder_rel)
        .filter_map(|copy| {
            let record = catalog.record(&copy.file_id)?;
            Some(ScannedFile {
                abs_path: PathBuf::new(),
                rel_path: copy.path[prefix_len..].to_string(),
                size: record.size,
                modified_ms: record.modified_at,
            })
        })
        .collect();
    files.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    files
}
