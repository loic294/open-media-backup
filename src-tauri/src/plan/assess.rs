use super::safe_copies::{device_safe_copy_report, SourceCopyFiles};
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
        let rules: Vec<(String, RuleSet)> = destinations
            .iter()
            .filter(|d| d.counts_as_safe_copy)
            .filter_map(|d| {
                let id = if d.kind == DestinationKind::App {
                    &d.id
                } else {
                    &d.device_id
                };
                Some((id.clone(), RuleSet::compile(&d.rules).ok()?))
            })
            .collect();
        let mut devices: Vec<Device> = store
            .list::<Device>()?
            .into_iter()
            .filter(|d| {
                d.role == DeviceRole::Final
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
        path_error: expanded
            .as_ref()
            .err()
            .map(|error| format!("Source path: {error}")),
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
    let report = safe_copy_report(
        &assessment.known,
        &targets,
        catalog,
        &vars,
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
    for (source, assessment) in &mut assessments {
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
    let enough = project.is_some_and(|p| report.safe_copies as u32 >= p.final_copies_required);
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
    } else if device.role == DeviceRole::Temporary && report.final_safe_copies == 0 {
        Some("Needs a final destination".into())
    } else if !enough {
        Some(match missing.as_slice() {
            [] => "No safe destination".to_string(),
            many => format!("Needs {}", many.join(", ")),
        })
    } else {
        None
    };
    assessment.status.safe_copies = report.safe_copies;
    assessment.status.wipe_eligible = blocking_reason.is_none() && source.offer_wipe;
    assessment.status.blocking_reason = blocking_reason;
}
