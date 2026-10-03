use crate::domain::{Device, Project, Space};
use crate::paths::{expand, read_backup_folder, TemplateVars};
use std::path::Path;

pub const DEFAULT_MARKER_TEMPLATE: &str = "{date}_{project_name}";

/// Space defaults, overridden by project values, plus `project` and `source_name` built-ins.
pub fn template_vars(space: &Space, project: &Project, source_device: &Device) -> TemplateVars {
    project_template_vars(space, Some(project), source_device)
}

pub fn project_template_vars(
    space: &Space,
    project: Option<&Project>,
    source_device: &Device,
) -> TemplateVars {
    let mut vars = TemplateVars::new();
    if let Some(project) = project {
        vars.insert("project".into(), project.name.clone());
        vars.insert("project_name".into(), project.name.clone());
    }
    vars.insert(
        "source_name".into(),
        super::sanitize_segment(&source_device.name),
    );
    if let Some(project) = project {
        for def in &space.variables {
            if !def.default_value.is_empty() {
                vars.insert(def.name.clone(), def.default_value.clone());
            }
        }
        for (name, value) in &project.values {
            if !value.is_empty() {
                vars.insert(name.clone(), value.clone());
            }
        }
    }
    vars
}

pub fn uses_project_variables(
    template: &str,
    space: &Space,
    projects: &[Project],
    marker: bool,
) -> bool {
    template
        .split('{')
        .skip(1)
        .filter_map(|part| part.split_once('}'))
        .any(|(name, _)| {
            let name = name.trim();
            if marker && name == "backup_folder" {
                return false;
            }
            matches!(name, "project" | "project_name")
                || space.variables.iter().any(|v| v.name == name)
                || projects.iter().any(|p| p.values.contains_key(name))
        })
}

pub fn validate_source_destination(
    source: &crate::domain::Source,
    destination: &crate::domain::Destination,
    space: &Space,
    projects: &[Project],
) -> Result<(), String> {
    if matches!(source.project_scope, crate::domain::ProjectScope::None)
        && uses_project_variables(
            &destination.path_template,
            space,
            projects,
            destination.use_backup_marker,
        )
    {
        return Err(
            "Source project scope is none; destination path cannot use project variables".into(),
        );
    }
    Ok(())
}

/// The `{backup_folder}` value from the source's marker, or what would be created.
pub fn backup_folder_name(
    space: &Space,
    vars: &TemplateVars,
    source_root: Option<&Path>,
) -> Result<String, String> {
    if let Some(existing) = source_root.and_then(read_backup_folder) {
        return Ok(existing);
    }
    let template = if space.backup_marker_template.trim().is_empty() {
        DEFAULT_MARKER_TEMPLATE
    } else {
        &space.backup_marker_template
    };
    expand(template, vars)
        .map(|s| super::sanitize_segment(&s))
        .map_err(|e| e.to_string())
}
