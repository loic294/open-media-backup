use super::safe_copies::{device_safe_copy_report, SafeCopyFile, SafeCopyState, SourceCopyFiles};
use super::{
    safe_copy_report, source_files, Catalog, FinalTarget, RootResolver, SafeCopyReport,
    SourceStatus,
};
use crate::domain::{
    Destination, DestinationKind, Device, DeviceKind, DeviceRole, Project, Source, Space,
};
use crate::paths::expand;
use crate::rules::RuleSet;
use crate::scan::ScannedFile;
use crate::store::{Store, StoreResult};
use std::path::PathBuf;

/// Final and temporary-role devices with rule sets from destinations that count as safe copies.
pub struct FinalSet {
    devices: Vec<Device>,
    rules: Vec<(String, RuleSet)>,
}

impl FinalSet {
    pub fn load(store: &Store) -> StoreResult<Self> {
        let destinations = store.list::<Destination>()?;
        Self::load_destinations(store, destinations, false)
    }

    fn load_destinations(
        store: &Store,
        destinations: Vec<Destination>,
        scoped: bool,
    ) -> StoreResult<Self> {
        let rules: Vec<(String, RuleSet)> = destinations
            .iter()
            .filter(|d| d.counts_as_safe_copy)
            .map(|d| {
                let id = if d.kind == DestinationKind::App {
                    &d.id
                } else {
                    &d.device_id
                };
                RuleSet::compile(&d.rules)
                    .map(|rules| (id.clone(), rules))
                    .map_err(|error| {
                        crate::store::StoreError::Invalid(format!("Destination {}: {error}", d.id))
                    })
            })
            .collect::<StoreResult<_>>()?;
        let mut devices: Vec<Device> = store
            .list::<Device>()?
            .into_iter()
            .filter(|d| {
                (d.role == DeviceRole::Final
                    && (!scoped || rules.iter().any(|(device_id, _)| device_id == &d.id)))
                    || (d.role == DeviceRole::Temporary
                        && rules.iter().any(|(device_id, _)| device_id == &d.id))
            })
            .collect();
        devices.extend(
            destinations
                .iter()
                .filter(|d| d.kind == DestinationKind::App && d.counts_as_safe_copy)
                .map(|destination| {
                    let app_name = destination
                        .app_name
                        .as_deref()
                        .map(str::trim)
                        .filter(|name| !name.is_empty())
                        .unwrap_or("Application");
                    let task_name = destination.task_name.trim();
                    Device {
                        id: destination.id.clone(),
                        name: if task_name.is_empty() {
                            app_name.to_owned()
                        } else {
                            task_name.to_owned()
                        },
                        description: app_name.to_owned(),
                        role: DeviceRole::Final,
                        kind: DeviceKind::Other,
                        ..Default::default()
                    }
                }),
        );
        Ok(Self { devices, rules })
    }

    pub fn load_for_space(store: &Store, space_id: &str) -> StoreResult<Self> {
        let destinations = store.list_by::<Destination>("space_id", space_id)?;
        Self::load_destinations(store, destinations, true)
    }

    pub fn targets(&self) -> Vec<FinalTarget<'_>> {
        self.devices
            .iter()
            .map(|device| FinalTarget {
                device,
                rules: self
                    .rules
                    .iter()
                    .filter(|(id, _)| *id == device.id)
                    .map(|(_, r)| r)
                    .collect(),
            })
            .collect()
    }
}

/// Everything known about one source: its files and how many safe copies they have.
pub struct SourceAssessment {
    pub status: SourceStatus,
    pub root: Option<PathBuf>,
    /// Source folder relative to the device root.
    pub folder: String,
    pub files: Vec<ScannedFile>,
    /// `(path relative to the source folder, known file id)` for every file.
    pub known: Vec<(String, Option<String>)>,
    /// Source-scoped coverage for wipe previews; status uses the shared device count.
    pub report: SafeCopyReport,
    pub device_files: Vec<SafeCopyFile>,
    safe_copy_rules: Option<RuleSet>,
    path_error: Option<String>,
}

impl SourceAssessment {
    pub fn device_path(&self, rel: &str) -> String {
        if self.folder.is_empty() {
            rel.to_string()
        } else {
            format!("{}/{rel}", self.folder)
        }
    }
}

pub fn assess_source(
    resolver: &dyn RootResolver,
    catalog: &Catalog,
    space: &Space,
    project: &Project,
    device: &Device,
    source: &Source,
    finals: &[FinalTarget],
) -> SourceAssessment {
    assess_workspace_source(
        resolver,
        catalog,
        space,
        Some(project),
        device,
        source,
        finals,
    )
}

pub fn assess_workspace_source(
    resolver: &dyn RootResolver,
    catalog: &Catalog,
    space: &Space,
    project: Option<&Project>,
    device: &Device,
    source: &Source,
    finals: &[FinalTarget],
) -> SourceAssessment {
    let root = resolver.device_root(&device.id).filter(|p| p.exists());
    let vars = super::source_template_vars(space, project, device, source);
    let expanded = expand(&source.path_template, &vars);
    let safe_copy_rules = RuleSet::compile(&source.safe_copy_rules);
    let folder = expanded
        .as_ref()
        .map(|s| s.trim_matches('/').to_string())
        .unwrap_or_default();
    let files = if expanded.is_ok() {
        source_files(root.as_deref(), &folder, &device.id, catalog)
    } else {
        Vec::new()
    };
    let mut assessment = SourceAssessment {
        status: SourceStatus {
            source_id: source.id.clone(),
            available: root.is_some(),
            root_path: root.as_ref().map(|p| p.display().to_string()),
            file_count: files.len(),
            total_bytes: files.iter().map(|f| f.size).sum(),
            safe_copies: 0,
            required_copies: project.map(|p| p.final_copies_required),
            wipe_eligible: false,
            blocking_reason: None,
        },
        root,
        folder,
        known: vec![],
        files: vec![],
        report: safe_copy_report(
            &[],
            &[],
            catalog,
            &vars,
            space.temporary_copies_per_final,
            space.skip_counts_as_safe_copy,
            &space.id,
        ),
        device_files: Vec::new(),
        path_error: safe_copy_rules
            .as_ref()
            .err()
            .map(|error| format!("Source safe-copy rules: {error}"))
            .or_else(|| {
                expanded
                    .as_ref()
                    .err()
                    .map(|error| format!("Source path: {error}"))
            }),
        safe_copy_rules: safe_copy_rules.ok(),
    };
    assessment.known = files
        .iter()
        .map(|f| {
            let path = assessment.device_path(&f.rel_path);
            (
                f.rel_path.clone(),
                catalog
                    .file_at(&device.id, &path, Some(f.size))
                    .map(|r| r.id.clone()),
            )
        })
        .collect();
    assessment.files = files;
    // Never count the source device as a copy of itself, and ignore temporary
    // destinations entirely unless the space lets them stand in for final copies.
    let targets: Vec<FinalTarget> = finals
        .iter()
        .filter(|t| t.device.id != device.id)
        .filter(|t| t.device.role == DeviceRole::Final || space.temporary_copies_per_final > 0)
        .map(|t| FinalTarget {
            device: t.device,
            rules: t.rules.clone(),
        })
        .collect();
    let report = device_safe_copy_report(
        &[SourceCopyFiles {
            folder: &assessment.folder,
            files: &assessment.known,
            vars: &vars,
            safe_copy_rules: assessment.safe_copy_rules.as_ref(),
        }],
        &targets,
        catalog,
        space.temporary_copies_per_final,
        space.skip_counts_as_safe_copy,
        &space.id,
    );
    apply_safety(&mut assessment, project, device, source, &report, None);
    assessment.report = report;
    assessment
}

/// Shares backup coverage across sources on one device without changing their file scopes.
pub fn assess_workspace_device(
    resolver: &dyn RootResolver,
    catalog: &Catalog,
    space: &Space,
    project: Option<&Project>,
    device: &Device,
    sources: &[Source],
    finals: &[FinalTarget],
) -> Vec<(Source, SourceAssessment)> {
    let mut assessments: Vec<_> = sources
        .iter()
        .filter(|source| source.space_id == space.id && source.device_id == device.id)
        .map(|source| {
            (
                source.clone(),
                assess_workspace_source(resolver, catalog, space, project, device, source, finals),
            )
        })
        .collect();
    let vars: Vec<_> = assessments
        .iter()
        .map(|(source, _)| super::source_template_vars(space, project, device, source))
        .collect();
    let files: Vec<_> = assessments
        .iter()
        .zip(&vars)
        .map(|((_, assessment), vars)| SourceCopyFiles {
            folder: &assessment.folder,
            files: &assessment.known,
            vars,
            safe_copy_rules: assessment.safe_copy_rules.as_ref(),
        })
        .collect();
    let targets: Vec<_> = finals
        .iter()
        .filter(|t| t.device.id != device.id)
        .filter(|t| t.device.role == DeviceRole::Final || space.temporary_copies_per_final > 0)
        .map(|t| FinalTarget {
            device: t.device,
            rules: t.rules.clone(),
        })
        .collect();
    let report = device_safe_copy_report(
        &files,
        &targets,
        catalog,
        space.temporary_copies_per_final,
        space.skip_counts_as_safe_copy,
        &space.id,
    );
    let path_error = assessments.iter().find_map(|(source, assessment)| {
        assessment
            .path_error
            .as_ref()
            .map(|error| format!("Source {}: {error}", source.id))
    });
    let scoped_reports: Vec<_> = assessments
        .iter()
        .map(|(_, assessment)| {
            let paths: std::collections::HashSet<_> = assessment
                .known
                .iter()
                .map(|(rel, _)| assessment.device_path(rel))
                .collect();
            let scoped_files: Vec<Vec<_>> = files
                .iter()
                .map(|source| {
                    source
                        .files
                        .iter()
                        .filter(|(rel, _)| {
                            let path = if source.folder.is_empty() {
                                rel.clone()
                            } else {
                                format!("{}/{rel}", source.folder)
                            };
                            paths.contains(&path)
                        })
                        .cloned()
                        .collect()
                })
                .collect();
            let scoped_views: Vec<_> = files
                .iter()
                .zip(&scoped_files)
                .map(|(source, known)| SourceCopyFiles {
                    folder: source.folder,
                    files: known,
                    vars: source.vars,
                    safe_copy_rules: source.safe_copy_rules,
                })
                .collect();
            device_safe_copy_report(
                &scoped_views,
                &targets,
                catalog,
                space.temporary_copies_per_final,
                space.skip_counts_as_safe_copy,
                &space.id,
            )
        })
        .collect();
    for ((source, assessment), scoped_report) in assessments.iter_mut().zip(scoped_reports) {
        assessment.report = scoped_report;
        apply_safety(
            assessment,
            project,
            device,
            source,
            &report,
            path_error.as_deref(),
        );
    }
    assessments
}

fn apply_safety(
    assessment: &mut SourceAssessment,
    project: Option<&Project>,
    device: &Device,
    source: &Source,
    report: &SafeCopyReport,
    group_error: Option<&str>,
) {
    let missing: Vec<&str> = report
        .copies
        .iter()
        .filter(|c| c.verified < c.total)
        .map(|c| c.device_name.as_str())
        .collect();
    let all_excluded = report.files_total > 0 && report.files_total == report.ignored;
    let uncovered = report.files.iter().any(|file| {
        file.state != SafeCopyState::Excluded
            && file.verified_destinations.is_empty()
            && file.acknowledged_destinations.is_empty()
            && file
                .reasons
                .iter()
                .any(|reason| reason.starts_with("No eligible"))
    });
    let unidentified = report.files.iter().any(|file| {
        file.state != SafeCopyState::Excluded
            && file
                .reasons
                .iter()
                .any(|reason| reason.starts_with("File identity is not verified"))
    });
    let enough = project
        .is_some_and(|p| all_excluded || report.safe_copies as u32 >= p.final_copies_required);
    let blocking_reason = if let Some(error) = group_error {
        Some(error.to_owned())
    } else if let Some(error) = &assessment.path_error {
        Some(error.clone())
    } else if project.is_none() {
        Some("Create a project to set card-wiping safety requirements".into())
    } else if assessment.root.is_none() {
        Some(format!("{} not mounted", device.name))
    } else if assessment.files.is_empty() {
        Some("No files".into())
    } else if device.role == DeviceRole::Final {
        Some("Final devices are never wiped".into())
    } else if device.role == DeviceRole::Temporary && !all_excluded && report.final_safe_copies == 0
    {
        Some("Needs a final destination".into())
    } else if uncovered {
        Some("Some files have no eligible safe-copy destination rule coverage".into())
    } else if !enough {
        Some(match missing.as_slice() {
            [] if report.copies.iter().any(|copy| copy.total > 0)
                && report.copies.iter().all(|copy| copy.total < report.files_total - report.ignored) =>
            {
                "No destination covers every required file; change destination rules or add a complete backup destination".to_string()
            }
            [] => "No safe destination".to_string(),
            many => format!("Needs {}", many.join(", ")),
        })
    } else if unidentified {
        Some("Some files have no verified identity; transfer or check them before wiping".into())
    } else {
        None
    };
    assessment.status.safe_copies = report.safe_copies;
    assessment.status.wipe_eligible = blocking_reason.is_none() && source.offer_wipe;
    assessment.status.blocking_reason = blocking_reason;
    assessment.device_files = report.files.clone();
    for file in &mut assessment.device_files {
        if file.state == SafeCopyState::Excluded {
            continue;
        }
        let threshold_met =
            project.is_some_and(|p| file.safe_copies as u32 >= p.final_copies_required);
        let identity_known = !file
            .reasons
            .iter()
            .any(|reason| reason.starts_with("File identity is not verified"));
        let has_coverage = !file
            .reasons
            .iter()
            .any(|reason| reason.starts_with("No eligible"));
        let needs_final = device.role == DeviceRole::Temporary && file.final_copies == 0;
        if threshold_met && identity_known && has_coverage && !needs_final {
            file.state = SafeCopyState::Safe;
        } else {
            if !threshold_met {
                file.reasons.push(match project {
                    Some(project) => format!(
                        "Needs {} safe copies; currently {}",
                        project.final_copies_required, file.safe_copies
                    ),
                    None => "Create or select a project to set safe-copy requirements".into(),
                });
            }
            if needs_final {
                file.reasons.push(
                    "Needs a final destination copy before this temporary device can be wiped"
                        .into(),
                );
            }
        }
    }
}
