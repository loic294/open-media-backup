use super::assess::{assess_source, FinalSet};
use super::{classify_files, resolve_flow, Catalog, Category, FailureMap, PlanError, RootResolver};
use crate::domain::{Destination, DestinationKind, Device, Flow, Project, Source, Space};
use crate::scan::ScannedFile;
use crate::store::Store;
use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowState {
    Done,
    Pending,
    Error,
    Unavailable,
    Empty,
}

#[derive(Debug, Clone, Serialize)]
pub struct FlowStatus {
    pub flow_id: String,
    pub state: FlowState,
    pub transferred: usize,
    pub to_transfer: usize,
    pub ignored: usize,
    pub failed: usize,
    pub bytes_to_transfer: u64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SourceStatus {
    pub source_id: String,
    pub available: bool,
    pub root_path: Option<String>,
    pub file_count: usize,
    pub total_bytes: u64,
    pub safe_copies: usize,
    pub required_copies: u32,
    pub wipe_eligible: bool,
    pub blocking_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct DestinationStatus {
    pub destination_id: String,
    pub available: bool,
    pub root_path: Option<String>,
    pub free_bytes: Option<u64>,
    pub transferred: usize,
    pub to_transfer: usize,
    pub ignored: usize,
    pub failed: usize,
    pub bytes_to_transfer: u64,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectStatus {
    pub project_id: String,
    pub flows: Vec<FlowStatus>,
    pub sources: Vec<SourceStatus>,
    pub destinations: Vec<DestinationStatus>,
}

pub fn project_status(
    store: &Store,
    resolver: &dyn RootResolver,
    catalog: &Catalog,
    project_id: &str,
    failures: &FailureMap,
) -> Result<ProjectStatus, PlanError> {
    let project: Project = store
        .get(project_id)?
        .ok_or_else(|| PlanError::NotFound(format!("project {project_id}")))?;
    let space: Space = store
        .get(&project.space_id)?
        .ok_or_else(|| PlanError::NotFound("space".into()))?;
    let devices: HashMap<String, Device> = store
        .list::<Device>()?
        .into_iter()
        .map(|d| (d.id.clone(), d))
        .collect();
    let final_set = FinalSet::load(store)?;
    let finals = final_set.targets();

    let mut sources: Vec<Source> = store.list_by("space_id", &space.id)?;
    sources.sort_by_key(|s| s.position);
    let mut files_by_source: HashMap<String, Vec<ScannedFile>> = HashMap::new();
    let mut source_statuses = Vec::new();
    for source in &sources {
        let Some(device) = devices.get(&source.device_id) else {
            continue;
        };
        let assessment =
            assess_source(resolver, catalog, &space, &project, device, source, &finals);
        source_statuses.push(assessment.status);
        let files = assessment.files;
        files_by_source.insert(source.id.clone(), files);
    }

    let flows: Vec<Flow> = store.list_by("space_id", &space.id)?;
    let mut dest_statuses: HashMap<String, DestinationStatus> = HashMap::new();
    let mut flow_statuses = Vec::new();
    for flow in &flows {
        let Ok(ctx) = resolve_flow(store, resolver, project_id, &flow.id) else {
            continue;
        };
        let files = files_by_source
            .get(&flow.source_id)
            .cloned()
            .unwrap_or_default();
        let planned = classify_files(&ctx, catalog, files, failures.get(&flow.id));
        let count = |c: Category| planned.iter().filter(|f| f.category == c).count();
        let (transferred, to_transfer, ignored, failed) = (
            count(Category::Transferred),
            count(Category::ToTransfer),
            count(Category::Ignored),
            count(Category::Error),
        );
        let bytes_to_transfer = planned
            .iter()
            .filter(|f| f.category == Category::ToTransfer)
            .map(|f| f.size)
            .sum();
        let available = ctx.source_root.is_some()
            && (ctx.destination.kind == DestinationKind::App || ctx.dest_root.is_some());
        let state = if ctx.config_error.is_some() || failed > 0 {
            FlowState::Error
        } else if to_transfer > 0 {
            if available {
                FlowState::Pending
            } else {
                FlowState::Unavailable
            }
        } else if transferred > 0 {
            FlowState::Done
        } else if ctx.destination.kind == DestinationKind::Folder && ctx.dest_root.is_none() {
            FlowState::Unavailable
        } else {
            FlowState::Empty
        };
        let error = ctx
            .config_error
            .clone()
            .or_else(|| planned.iter().find_map(|f| f.error.clone()));
        let dest = dest_statuses
            .entry(ctx.destination.id.clone())
            .or_insert_with(|| DestinationStatus {
                destination_id: ctx.destination.id.clone(),
                available: ctx.destination.kind == DestinationKind::App || ctx.dest_root.is_some(),
                root_path: if ctx.destination.kind == DestinationKind::App {
                    None
                } else {
                    ctx.dest_root.as_ref().map(|p| p.display().to_string())
                },
                ..Default::default()
            });
        dest.transferred += transferred;
        dest.to_transfer += to_transfer;
        dest.ignored += ignored;
        dest.failed += failed;
        dest.bytes_to_transfer += bytes_to_transfer;
        if dest.last_error.is_none() {
            dest.last_error = error.clone();
        }
        flow_statuses.push(FlowStatus {
            flow_id: flow.id.clone(),
            state,
            transferred,
            to_transfer,
            ignored,
            failed,
            bytes_to_transfer,
            error,
        });
    }
    for dest in store.list_by::<Destination>("space_id", &space.id)? {
        dest_statuses.entry(dest.id.clone()).or_insert_with(|| {
            let root = (dest.kind == DestinationKind::Folder)
                .then(|| resolver.device_root(&dest.device_id).filter(|p| p.exists()))
                .flatten();
            DestinationStatus {
                destination_id: dest.id.clone(),
                available: dest.kind == DestinationKind::App || root.is_some(),
                root_path: if dest.kind == DestinationKind::App {
                    None
                } else {
                    root.map(|p| p.display().to_string())
                },
                ..Default::default()
            }
        });
    }
    Ok(ProjectStatus {
        project_id: project_id.to_string(),
        flows: flow_statuses,
        sources: source_statuses,
        destinations: dest_statuses.into_values().collect(),
    })
}
