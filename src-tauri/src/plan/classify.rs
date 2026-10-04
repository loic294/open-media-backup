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
    /// Unix milliseconds from embedded metadata, never filesystem modification time.
    pub capture_time: Option<i64>,
    pub project_id: Option<String>,
    /// Already expanded using this row's project, relative to the destination device.
    pub target_path: Option<String>,
}

impl PlannedFile {
    pub fn failure_key(&self) -> String {
        failure_key(&self.rel_path, self.project_id.as_deref())
    }
}

fn failure_key(rel: &str, project_id: Option<&str>) -> String {
    match project_id {
        Some(id) => serde_json::to_string(&(id, rel)).expect("string pairs serialize"),
        None => rel.to_string(),
    }
}

/// Classifies every file of a flow's source. Uses the catalog when the source is offline.
pub fn classify_flow(
    ctx: &FlowContext,
    catalog: &Catalog,
    failures: Option<&HashMap<String, String>>,
) -> Vec<PlannedFile> {
    if !ctx.source_path_valid {
        return Vec::new();
    }
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
    let mut capture_times = HashMap::new();
    let mut errors = failures.cloned().unwrap_or_default();
    let needs_capture = ctx.source_root.is_some()
        && ctx.projects.iter().any(|project| {
            ctx.source.project_scope.allows(&project.id)
                && project.start_time.is_some()
                && project.end_time.is_some()
        });
    if needs_capture {
        for file in &files {
            match crate::metadata::extract_metadata(&file.abs_path) {
                Ok(metadata) => {
                    if let Some(capture) = metadata.capture_time {
                        match capture_time_ms(&capture) {
                            Ok(Some(time)) => {
                                capture_times.insert(file.rel_path.clone(), time);
                            }
                            Ok(None) => {}
                            Err(error) => {
                                errors.insert(file.rel_path.clone(), error);
                            }
                        }
                    }
                }
                Err(crate::metadata::MetadataError::Unsupported { .. }) => {}
                Err(error) => {
                    errors.insert(file.rel_path.clone(), format!("capture metadata: {error}"));
                }
            }
        }
    }
    classify_files_with_capture_times(ctx, catalog, files, &capture_times, Some(&errors))
}

/// Unknown-zone wall times cannot be matched to UTC project bounds without a timezone policy.
pub fn capture_time_ms(capture: &crate::metadata::CaptureTime) -> Result<Option<i64>, String> {
    let Some(offset) = capture.utc_offset_seconds else {
        return Ok(None);
    };
    let local = capture
        .local_datetime
        .parse::<chrono::NaiveDateTime>()
        .map_err(|error| format!("capture metadata: invalid capture time: {error}"))?;
    local
        .and_utc()
        .timestamp_millis()
        .checked_sub(i64::from(offset) * 1000)
        .map(Some)
        .ok_or_else(|| "capture metadata: timestamp outside supported range".into())
}

/// Metadata extraction is supplied separately from scanning. Absent entries are unassigned.
pub fn classify_files_with_capture_times(
    ctx: &FlowContext,
    catalog: &Catalog,
    files: Vec<ScannedFile>,
    capture_times: &HashMap<String, i64>,
    failures: Option<&HashMap<String, String>>,
) -> Vec<PlannedFile> {
    files
        .into_iter()
        .flat_map(|file| {
            let device_path = ctx.source_device_path(&file.rel_path);
            let known = catalog.file_at(&ctx.source_device.id, &device_path, Some(file.size));
            let file_id = known.map(|r| r.id.clone());
            let capture_time = capture_times.get(&file.rel_path).copied();
            let matches = ctx.matching_projects(capture_time);
            let projects: Vec<_> = if matches.is_empty() {
                vec![None]
            } else {
                matches.into_iter().map(Some).collect()
            };
            projects
                .into_iter()
                .map(|project| {
                    let target = ctx.target_for_project(&file.rel_path, project);
                    let rule_vars = ctx.rule_vars_for_project(project);
                    let configuration_failed = ctx.config_error.is_some() || target.is_err();
                    let error = ctx
                        .config_error
                        .clone()
                        .or_else(|| target.as_ref().err().cloned())
                        .or_else(|| {
                            failures
                                .and_then(|f| {
                                    f.get(&failure_key(
                                        &file.rel_path,
                                        project.map(|p| p.id.as_str()),
                                    ))
                                    .or_else(|| f.get(&file.rel_path))
                                })
                                .cloned()
                        });
                    let target_path = target.ok().flatten();
                    let category = if !ctx.rules.allows_with_vars(&file.rel_path, &rule_vars) {
                        Category::Ignored
                    } else if configuration_failed {
                        Category::Error
                    } else if target_path.is_none() {
                        if error.is_some() {
                            Category::Error
                        } else {
                            Category::Ignored
                        }
                    } else if file_id.as_ref().is_some_and(|id| {
                        if project.is_some() {
                            target_path.as_ref().is_some_and(|path| {
                                catalog.has_copy_for_target(id, &ctx.dest_device.id, path)
                            })
                        } else {
                            catalog.has_copy_on(id, &ctx.dest_device.id)
                        }
                    }) {
                        Category::Transferred
                    } else if error.is_some() {
                        Category::Error
                    } else {
                        Category::ToTransfer
                    };
                    PlannedFile {
                        rel_path: file.rel_path.clone(),
                        abs_path: ctx.source_root.is_some().then_some(file.abs_path.clone()),
                        size: file.size,
                        modified_ms: file.modified_ms,
                        file_id: file_id.clone(),
                        category,
                        capture_time,
                        project_id: project.map(|p| p.id.clone()),
                        target_path: (category != Category::Ignored)
                            .then_some(target_path)
                            .flatten(),
                        error: if category == Category::Error {
                            error
                        } else {
                            None
                        },
                    }
                })
                .collect::<Vec<_>>()
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
