use super::{safe_copy_report, source_files, template_vars, Catalog, FinalTarget, RootResolver, SafeCopyReport, SourceStatus};
use crate::domain::{Destination, Device, DeviceRole, Project, Source, Space};
use crate::paths::expand;
use crate::rules::RuleSet;
use crate::scan::ScannedFile;
use crate::store::{Store, StoreResult};
use std::path::PathBuf;

/// Final-role devices with the rule sets of the destinations that count as safe copies.
pub struct FinalSet {
    devices: Vec<Device>,
    rules: Vec<(String, RuleSet)>,
}

impl FinalSet {
    pub fn load(store: &Store) -> StoreResult<Self> {
        let devices = store.list::<Device>()?.into_iter().filter(|d| d.role == DeviceRole::Final).collect();
        let rules = store
            .list::<Destination>()?
            .into_iter()
            .filter(|d| d.counts_as_safe_copy)
            .filter_map(|d| Some((d.device_id.clone(), RuleSet::compile(&d.rules).ok()?)))
            .collect();
        Ok(Self { devices, rules })
    }

    pub fn targets(&self) -> Vec<FinalTarget<'_>> {
        self.devices
            .iter()
            .map(|device| FinalTarget {
                device,
                rules: self.rules.iter().filter(|(id, _)| *id == device.id).map(|(_, r)| r).collect(),
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
    pub report: SafeCopyReport,
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
    let root = resolver.device_root(&device.id).filter(|p| p.exists());
    let vars = template_vars(space, project, device);
    let folder = expand(&source.path_template, &vars).unwrap_or_default().trim_matches('/').to_string();
    let files = source_files(root.as_deref(), &folder, &device.id, catalog);
    let mut assessment = SourceAssessment {
        status: SourceStatus {
            source_id: source.id.clone(),
            available: root.is_some(),
            root_path: root.as_ref().map(|p| p.display().to_string()),
            file_count: files.len(),
            total_bytes: files.iter().map(|f| f.size).sum(),
            safe_copies: 0,
            required_copies: project.final_copies_required,
            wipe_eligible: false,
            blocking_reason: None,
        },
        root,
        folder,
        known: vec![],
        files: vec![],
        report: safe_copy_report(&[], &[], catalog),
    };
    assessment.known = files
        .iter()
        .map(|f| {
            let path = assessment.device_path(&f.rel_path);
            (f.rel_path.clone(), catalog.file_at(&device.id, &path, Some(f.size)).map(|r| r.id.clone()))
        })
        .collect();
    assessment.files = files;
    let report = safe_copy_report(&assessment.known, finals, catalog);
    let missing: Vec<&str> = report.copies.iter().filter(|c| c.verified < c.total).map(|c| c.device_name.as_str()).collect();
    let enough = report.safe_copies as u32 >= project.final_copies_required;
    let blocking_reason = if assessment.root.is_none() {
        Some(format!("{} not mounted", device.name))
    } else if assessment.files.is_empty() {
        Some("No files".into())
    } else if device.role == DeviceRole::Final {
        Some("Final devices are never wiped".into())
    } else if !enough {
        Some(match missing.as_slice() {
            [] => "No final destination".to_string(),
            many => format!("Needs {}", many.join(", ")),
        })
    } else {
        None
    };
    assessment.status.safe_copies = report.safe_copies;
    assessment.status.wipe_eligible = blocking_reason.is_none() && source.offer_wipe;
    assessment.status.blocking_reason = blocking_reason;
    assessment.report = report;
    assessment
}
