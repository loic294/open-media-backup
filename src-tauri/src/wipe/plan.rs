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
    let sources = store
        .list_by::<Source>("space_id", &space.id)
        .map_err(|e| e.to_string())?;
    let assessment = assess_workspace_device(
        resolver,
        &catalog,
        &space,
        Some(&project),
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
    let source: Source = store
        .get(source_id)
        .map_err(|e| e.to_string())?
        .ok_or("source not found")?;
    let projects: Vec<Project> = store
        .list_by::<Project>("space_id", &source.space_id)
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter(|project| !project.archived)
        .collect();
    if projects.is_empty() {
        return Ok(WipePlan {
            source_id: source_id.to_string(),
            files_total: 0,
            ignored: 0,
            copies: Vec::new(),
            eligible: false,
            reason: Some("No active project applies to this source".into()),
        });
    }

    let mut plans = Vec::with_capacity(projects.len());
    let mut folder = None;
    let mut different_source_folders = false;
    for project in &projects {
        let assessed = assess(store, resolver, &project.id, source_id)?;
        if folder
            .as_ref()
            .is_some_and(|folder: &String| folder != &assessed.assessment.folder)
        {
            different_source_folders = true;
        } else {
            folder = Some(assessed.assessment.folder.clone());
        }
        plans.push((project, plan_wipe(store, resolver, &project.id, source_id)?));
    }

    let mut copies = plans[0].1.copies.clone();
    for copy in &mut copies {
        for (_, plan) in &plans[1..] {
            if let Some(other) = plan
                .copies
                .iter()
                .find(|item| item.device_id == copy.device_id)
            {
                copy.verified = copy.verified.min(other.verified);
                copy.total = copy.total.max(other.total);
            }
        }
    }
    let reason = if different_source_folders {
        Some("Source path resolves to different folders across active projects".into())
    } else {
        plans
            .iter()
            .find(|(_, plan)| !plan.eligible)
            .and_then(|(project, plan)| {
                plan.reason
                    .as_ref()
                    .map(|reason| format!("{}: {reason}", project.name))
            })
    };
    Ok(WipePlan {
        source_id: source_id.to_string(),
        files_total: plans
            .iter()
            .map(|(_, plan)| plan.files_total)
            .max()
            .unwrap_or(0),
        ignored: plans
            .iter()
            .map(|(_, plan)| plan.ignored)
            .max()
            .unwrap_or(0),
        copies,
        eligible: reason.is_none(),
        reason,
    })
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
    let source: Source = store
        .get(source_id)
        .map_err(|e| e.to_string())?
        .ok_or("source not found")?;
    let project: Project = store
        .list_by::<Project>("space_id", &source.space_id)
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|project| !project.archived)
        .ok_or("No active project applies to this source")?;
    super::run::wipe(store, resolver, &project.id, source_id, method, handle)
}
