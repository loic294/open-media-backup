use crate::domain::{Device, Project, Source, Space};
use crate::plan::{
    assess_workspace_device, Catalog, DeviceCopies, FinalSet, RootResolver, SourceAssessment,
};
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
    let project: Option<Project> = if project_id.is_empty() {
        None
    } else {
        Some(
            store
                .get(project_id)
                .map_err(|e| e.to_string())?
                .ok_or("project not found")?,
        )
    };
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
    if project
        .as_ref()
        .is_some_and(|project| project.space_id != space.id)
    {
        return Err("source and project belong to different spaces".into());
    }
    let catalog = Catalog::load(store).map_err(|e| e.to_string())?;
    let finals = FinalSet::load_for_space(store, &space.id).map_err(|e| e.to_string())?;
    let sources = store
        .list_by::<Source>("space_id", &space.id)
        .map_err(|e| e.to_string())?;
    let assessment = assess_workspace_device(
        resolver,
        &catalog,
        &space,
        project.as_ref(),
        &device,
        &sources,
        &finals.targets(),
    )
    .into_iter()
    .find(|(candidate, _)| candidate.id == source.id)
    .map(|(_, assessment)| assessment)
    .ok_or("source not found in device assessment")?;
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

pub fn plan_workspace_wipe(
    store: &Store,
    resolver: &dyn RootResolver,
    source_id: &str,
) -> Result<WipePlan, String> {
    plan_wipe(store, resolver, "", source_id)
}

pub fn wipe_workspace(
    store: &Store,
    resolver: &dyn RootResolver,
    source_id: &str,
    method: super::WipeMethod,
    handle: &crate::transfer::JobHandle,
) -> Result<(), String> {
    let plan = plan_workspace_wipe(store, resolver, source_id)?;
    if let Some(reason) = plan.reason {
        return Err(reason);
    }
    super::run::wipe_preserving(
        store,
        resolver,
        "",
        source_id,
        method,
        handle,
        &Default::default(),
    )
}
