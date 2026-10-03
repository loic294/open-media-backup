use crate::domain::{Device, Project, Space};
use crate::paths::{expand, read_backup_folder, TemplateVars};
use std::path::Path;

pub const DEFAULT_MARKER_TEMPLATE: &str = "{date}_{project_name}";

/// Space defaults, overridden by project values, plus `project` and `source_name` built-ins.
pub fn template_vars(space: &Space, project: &Project, source_device: &Device) -> TemplateVars {
    let mut vars = TemplateVars::new();
    vars.insert("project".into(), project.name.clone());
    vars.insert("project_name".into(), project.name.clone());
    vars.insert(
        "source_name".into(),
        super::sanitize_segment(&source_device.name),
    );
    for def in &space.variables {
        vars.insert(def.name.clone(), def.default_value.clone());
    }
    for (name, value) in &project.values {
        if !value.is_empty() {
            vars.insert(name.clone(), value.clone());
        }
    }
    vars
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
