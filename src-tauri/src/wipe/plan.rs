use crate::domain::{Device, Project, Source, Space};
use crate::plan::{assess_source, Catalog, DeviceCopies, FinalSet, RootResolver, SourceAssessment};
use crate::store::Store;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct WipePlan {
    pub source_id: String,
    pub files_total: usize,
    pub ignored: usize,
    pub copies: Vec<DeviceCopies>,
    pub eligible: bool,
    pub reason: Option<String>,
}

pub struct Assessed {
    pub source: Source,
    pub device: Device,
    pub catalog: Catalog,
    pub assessment: SourceAssessment,
}

pub fn assess(
    store: &Store,
    resolver: &dyn RootResolver,
    project_id: &str,
    source_id: &str,
) -> Result<Assessed, String> {
    let project: Project = store
        .get(project_id)
        .map_err(|e| e.to_string())?
        .ok_or("project not found")?;
    let source: Source = store
        .get(source_id)
        .map_err(|e| e.to_string())?
        .ok_or("source not found")?;
    let space: Space = store
        .get(&source.space_id)
        .map_err(|e| e.to_string())?
        .ok_or("space not found")?;
    let device: Device = store
        .get(&source.device_id)
        .map_err(|e| e.to_string())?
        .ok_or("device not found")?;
    if project.space_id != space.id {
        return Err("source and project belong to different spaces".into());
    }
    let catalog = Catalog::load(store).map_err(|e| e.to_string())?;
    let finals = FinalSet::load(store).map_err(|e| e.to_string())?;
    let assessment = assess_source(
        resolver,
        &catalog,
        &space,
        &project,
        &device,
        &source,
        &finals.targets(),
    );
    Ok(Assessed {
        source,
        device,
        catalog,
        assessment,
    })
}

pub fn plan_wipe(
    store: &Store,
    resolver: &dyn RootResolver,
    project_id: &str,
    source_id: &str,
) -> Result<WipePlan, String> {
    let Assessed {
        assessment, source, ..
    } = assess(store, resolver, project_id, source_id)?;
    let reason = match &assessment.status.blocking_reason {
        Some(r) => Some(r.clone()),
        None if !source.offer_wipe => Some("Wiping is disabled for this source".into()),
        None => None,
    };
    Ok(WipePlan {
        source_id: source_id.to_string(),
        files_total: assessment.report.files_total,
        ignored: assessment.report.ignored,
        copies: assessment.report.copies,
        eligible: reason.is_none(),
        reason,
    })
}
