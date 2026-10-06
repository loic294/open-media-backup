use super::{backup_folder_name, sanitize_segment, RootResolver};
use crate::domain::{
    validate_project_ranges, Destination, DestinationKind, Device, DeviceKind, DeviceRole, Flow,
    Project, Source, Space,
};
use crate::paths::{expand, join_relative, TemplateVars};
use crate::rules::RuleSet;
use crate::store::{Store, StoreError};
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PlanError {
    #[error("{0} not found")]
    NotFound(String),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("{0}")]
    InvalidContext(String),
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceContext {
    pub space_id: String,
    pub project_id: Option<String>,
}

impl WorkspaceContext {
    pub fn for_project(store: &Store, project_id: &str) -> Result<Self, PlanError> {
        let project: Project = load(store, project_id, "project")?;
        Ok(Self {
            space_id: project.space_id,
            project_id: Some(project.id),
        })
    }

    pub fn load(&self, store: &Store) -> Result<(Space, Option<Project>), PlanError> {
        let space: Space = load(store, &self.space_id, "space")?;
        let project: Option<Project> = self
            .project_id
            .as_deref()
            .map(|id| load(store, id, "project"))
            .transpose()?;
        if project.as_ref().is_some_and(|p| p.space_id != space.id) {
            return Err(PlanError::InvalidContext(
                "Project must belong to the workspace space".into(),
            ));
        }
        Ok((space, project))
    }
}

/// Everything needed to plan or run one source → destination flow.
pub struct FlowContext {
    pub destination_hasher: crate::transfer::destination_hasher::DestinationHasher,
    pub flow: Flow,
    pub space: Space,
    pub project: Option<Project>,
    pub projects: Vec<Project>,
    pub source: Source,
    pub destination: Destination,
    pub source_device: Device,
    pub dest_device: Device,
    pub vars: TemplateVars,
    pub source_root: Option<PathBuf>,
    pub dest_root: Option<PathBuf>,
    /// Source folder relative to the source device root.
    pub source_folder_rel: String,
    pub source_path_valid: bool,
    /// Target base folder relative to the destination device root.
    pub dest_folder_rel: String,
    pub rules: RuleSet,
    /// Template or rule problems that block transfers.
    pub config_error: Option<String>,
}

fn load<E: crate::domain::Entity>(store: &Store, id: &str, what: &str) -> Result<E, PlanError> {
    store
        .get::<E>(id)?
        .ok_or_else(|| PlanError::NotFound(format!("{what} {id}")))
}

fn load_task_device(store: &Store, id: &str, task: &str) -> Result<Device, PlanError> {
    store.get::<Device>(id)?.ok_or_else(|| {
        PlanError::InvalidContext(format!("Select a device for this {task} in its settings"))
    })
}

pub fn resolve_flow(
    store: &Store,
    resolver: &dyn RootResolver,
    project_id: &str,
    flow_id: &str,
) -> Result<FlowContext, PlanError> {
    let context = WorkspaceContext::for_project(store, project_id)?;
    resolve_workspace_flow(store, resolver, &context, flow_id)
}

pub fn resolve_workspace_flow(
    store: &Store,
    resolver: &dyn RootResolver,
    context: &WorkspaceContext,
    flow_id: &str,
) -> Result<FlowContext, PlanError> {
    let (space, project) = context.load(store)?;
    let flow: Flow = load(store, flow_id, "flow")?;
    if flow.space_id != space.id {
        return Err(PlanError::InvalidContext(
            "Flow must belong to the workspace space".into(),
        ));
    }
    let source: Source = load(store, &flow.source_id, "source")?;
    let destination: Destination = load(store, &flow.destination_id, "destination")?;
    if source.space_id != space.id || destination.space_id != space.id {
        return Err(PlanError::InvalidContext(
            "Flow entities must belong to the same space".into(),
        ));
    }
    let source_device = load_task_device(store, &source.device_id, "source")?;
    let dest_device: Device = if destination.kind == DestinationKind::App {
        Device {
            id: destination.id.clone(),
            name: app_destination_name(&destination),
            description: destination
                .app_name
                .clone()
                .unwrap_or_else(|| "Application".into()),
            role: DeviceRole::Temporary,
            kind: DeviceKind::Other,
            ..Default::default()
        }
    } else {
        load_task_device(store, &destination.device_id, "destination")?
    };
    let source_root = resolver
        .device_root(&source.device_id)
        .filter(|p| p.exists());
    let dest_root = (destination.kind == DestinationKind::Folder)
        .then(|| {
            resolver
                .device_root(&destination.device_id)
                .filter(|p| p.exists())
        })
        .flatten();

    let mut errors = Vec::new();
    let all_projects: Vec<Project> = store.list_by::<Project>("space_id", &space.id)?;
    let projects: Vec<Project> = all_projects
        .iter()
        .filter(|project| !project.archived)
        .cloned()
        .collect();
    if let Err(error) = validate_project_ranges(&all_projects, space.allow_project_overlap) {
        errors.push(error);
    }
    if let Err(error) = super::validate_source_destination(&source, &destination, &space, &projects)
    {
        errors.push(error);
    }
    if let Err(error) = source.validate_projects(&all_projects) {
        errors.push(error);
    }
    let mut vars = if matches!(source.project_scope, crate::domain::ProjectScope::None) {
        super::source_template_vars(&space, None, &source_device, &source)
    } else {
        super::source_template_vars(&space, project.as_ref(), &source_device, &source)
    };
    if project.is_none()
        && projects.is_empty()
        && destination.kind == DestinationKind::Folder
        && super::uses_project_variables(
            &destination.path_template,
            &space,
            &projects,
            destination.use_backup_marker,
        )
    {
        errors.push("Destination path requires project values; create a project or use a project-independent path".into());
    }
    if destination.use_backup_marker {
        match backup_folder_name(&space, &vars, source_root.as_deref()) {
            Ok(folder) => drop(vars.insert("backup_folder".into(), folder)),
            Err(e) => errors.push(format!("backup folder: {e}")),
        }
    }
    let expanded_source = expand(&source.path_template, &vars);
    let source_path_valid = expanded_source.is_ok();
    let source_folder_rel = normalize(&expanded_source.unwrap_or_else(|e| {
        errors.push(format!("source path: {e}"));
        String::new()
    }));
    // The selected project's preview is retained for old callers. Actual routing below
    // expands the destination separately for each file's matching projects.
    let mut dest_folder_rel = if destination.kind == DestinationKind::App {
        String::new()
    } else {
        normalize(&expand(&destination.path_template, &vars).unwrap_or_default())
    };
    let mut validation_vars = vars.clone();
    for name in ["project", "project_name"] {
        validation_vars.insert(name.into(), "validation".into());
    }
    for name in space
        .variables
        .iter()
        .map(|v| &v.name)
        .chain(projects.iter().flat_map(|p| p.values.keys()))
    {
        validation_vars.insert(name.clone(), "validation".into());
    }
    if destination.kind == DestinationKind::Folder {
        if let Err(error) = expand(&destination.path_template, &validation_vars) {
            errors.push(format!("destination path: {error}"));
        }
    }
    if destination.kind == DestinationKind::Folder && destination.subfolder_per_source {
        let segment = sanitize_segment(source.resolved_backup_name(&source_device));
        dest_folder_rel = if dest_folder_rel.is_empty() {
            segment
        } else {
            format!("{dest_folder_rel}/{segment}")
        };
    }
    let rules = RuleSet::compile(&destination.rules).unwrap_or_else(|e| {
        errors.push(e.to_string());
        RuleSet::compile(&[]).expect("empty rule set")
    });
    Ok(FlowContext {
        destination_hasher: Default::default(),
        flow,
        space,
        project,
        projects,
        source,
        destination,
        source_device,
        dest_device,
        vars,
        source_root,
        dest_root,
        source_folder_rel,
        source_path_valid,
        dest_folder_rel,
        rules,
        config_error: (!errors.is_empty()).then(|| errors.join("; ")),
    })
}

fn normalize(path: &str) -> String {
    path.split(['/', '\\'])
        .filter(|p| !p.is_empty() && *p != "." && *p != "..")
        .collect::<Vec<_>>()
        .join("/")
}

impl FlowContext {
    /// Every eligible project is included; a missing embedded timestamp never assigns one.
    pub fn matching_projects(&self, capture_time: Option<i64>) -> Vec<&Project> {
        self.projects
            .iter()
            .filter(|project| {
                !project.archived
                    && self.source.project_scope.allows(&project.id)
                    && project.matches_capture_time(capture_time)
            })
            .collect()
    }

    pub fn target_for_project(
        &self,
        rel: &str,
        project: Option<&Project>,
    ) -> Result<Option<String>, String> {
        if self.destination.kind == DestinationKind::App {
            return Ok(Some(match project {
                Some(project) => join_rel(&project.id, rel),
                None => rel.to_string(),
            }));
        }
        if project.is_none()
            && super::uses_project_variables(
                &self.destination.path_template,
                &self.space,
                &self.projects,
                self.destination.use_backup_marker,
            )
        {
            return Ok(None);
        }
        let mut vars =
            super::source_template_vars(&self.space, project, &self.source_device, &self.source);
        if self.destination.use_backup_marker {
            let folder = backup_folder_name(&self.space, &vars, self.source_root.as_deref())
                .map_err(|e| format!("backup folder: {e}"))?;
            vars.insert("backup_folder".into(), folder);
        }
        let mut folder = normalize(
            &expand(&self.destination.path_template, &vars)
                .map_err(|e| format!("destination path: {e}"))?,
        );
        if self.destination.subfolder_per_source {
            folder = join_rel(
                &folder,
                &sanitize_segment(self.source.resolved_backup_name(&self.source_device)),
            );
        }
        let file_path = if self.destination.preserve_file_structure {
            rel
        } else {
            rel.rsplit('/').next().unwrap_or(rel)
        };
        Ok(Some(join_rel(&folder, file_path)))
    }

    /// Variables used by condition rules for a single routed file. Condition
    /// rules are project filters, so unassigned files intentionally get no vars.
    pub fn rule_vars_for_project(&self, project: Option<&Project>) -> TemplateVars {
        project
            .map(|project| {
                super::source_template_vars(
                    &self.space,
                    Some(project),
                    &self.source_device,
                    &self.source,
                )
            })
            .unwrap_or_default()
    }

    pub fn source_folder(&self) -> Option<PathBuf> {
        if !self.source_path_valid {
            return None;
        }
        self.source_root
            .as_ref()
            .map(|r| join_relative(r, &self.source_folder_rel))
    }

    /// Path relative to the source device root for a path relative to the source folder.
    pub fn source_device_path(&self, rel: &str) -> String {
        join_rel(&self.source_folder_rel, rel)
    }

    /// Path relative to the destination device root.
    pub fn target_rel(&self, rel: &str) -> String {
        join_rel(&self.dest_folder_rel, rel)
    }

    pub fn target_abs(&self, rel: &str) -> Option<PathBuf> {
        self.dest_root
            .as_ref()
            .map(|r| join_relative(r, &self.target_rel(rel)))
    }

    pub fn label(&self) -> String {
        format!(
            "{} → {}",
            self.source.resolved_task_name(&self.source_device),
            self.destination.resolved_task_name(&self.dest_device)
        )
    }
}

fn app_destination_name(destination: &Destination) -> String {
    destination
        .app_name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| "Application".into())
}

fn join_rel(base: &str, rel: &str) -> String {
    if base.is_empty() {
        rel.to_string()
    } else {
        format!("{base}/{rel}")
    }
}
