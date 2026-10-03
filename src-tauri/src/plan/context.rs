use super::{backup_folder_name, sanitize_segment, template_vars, RootResolver};
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
}

/// Everything needed to plan or run one source → destination flow.
pub struct FlowContext {
    pub flow: Flow,
    pub space: Space,
    pub project: Project,
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

pub fn resolve_flow(
    store: &Store,
    resolver: &dyn RootResolver,
    project_id: &str,
    flow_id: &str,
) -> Result<FlowContext, PlanError> {
    let flow: Flow = load(store, flow_id, "flow")?;
    let project: Project = load(store, project_id, "project")?;
    let space: Space = load(store, &flow.space_id, "space")?;
    let source: Source = load(store, &flow.source_id, "source")?;
    let destination: Destination = load(store, &flow.destination_id, "destination")?;
    let source_device: Device = load(store, &source.device_id, "device")?;
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
        load(store, &destination.device_id, "device")?
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
    let projects: Vec<Project> = store.list_by("space_id", &space.id)?;
    if project.space_id != space.id
        || source.space_id != space.id
        || destination.space_id != space.id
    {
        errors.push("Flow entities must belong to the same space".into());
    }
    if let Err(error) = validate_project_ranges(&projects, space.allow_project_overlap) {
        errors.push(error);
    }
    if let Err(error) = super::validate_source_destination(&source, &destination, &space, &projects)
    {
        errors.push(error);
    }
    if let Err(error) = source.validate_projects(&projects) {
        errors.push(error);
    }
    let mut vars = if matches!(source.project_scope, crate::domain::ProjectScope::None) {
        super::project_template_vars(&space, None, &source_device)
    } else {
        template_vars(&space, &project, &source_device)
    };
    if destination.use_backup_marker {
        match backup_folder_name(&space, &vars, source_root.as_deref()) {
            Ok(folder) => drop(vars.insert("backup_folder".into(), folder)),
            Err(e) => errors.push(format!("backup folder: {e}")),
        }
    }
    let mut expand_or_note = |template: &str, what: &str| {
        expand(template, &vars).unwrap_or_else(|e| {
            errors.push(format!("{what}: {e}"));
            String::new()
        })
    };
    let source_folder_rel = normalize(&expand_or_note(&source.path_template, "source path"));
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
        let segment = sanitize_segment(&source_device.name);
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
                self.source.project_scope.allows(&project.id)
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
        let mut vars = super::project_template_vars(&self.space, project, &self.source_device);
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
            folder = join_rel(&folder, &sanitize_segment(&self.source_device.name));
        }
        Ok(Some(join_rel(&folder, rel)))
    }

    pub fn source_folder(&self) -> Option<PathBuf> {
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
        format!("{} → {}", self.source_device.name, self.dest_device.name)
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
