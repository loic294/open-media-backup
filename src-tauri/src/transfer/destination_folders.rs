//! Resolves the existing folders of a folder destination from the database.
//!
//! Each incoming flow's template is expanded for every project in the space (and
//! without a project when the template is project-independent). Fully resolved
//! templates give folders directly. Values that cannot be known offline (a source
//! card's `{backup_folder}`, date built-ins, missing variables) match exactly one
//! path segment and are taken from catalog paths recorded on the destination device.
//! A connected source's backup marker also gives its current `{backup_folder}`.

use crate::domain::{Destination, Device, Flow, Project, Source, Space};
use crate::paths::{read_backup_folder, TemplateVars};
use crate::plan::{
    project_template_vars, source_template_vars, uses_project_variables, RootResolver,
};
use crate::store::Store;
use regex::Regex;
use std::collections::BTreeSet;

const UNKNOWN: char = '\u{1}';

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Segment {
    Literal(String),
    /// Literal parts separated by unknown values, each matching one or more characters.
    Pattern(Vec<String>),
}

fn segments(template: &str, vars: &TemplateVars) -> Option<Vec<Segment>> {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let end = after.find('}')?;
        match vars.get(after[..end].trim()).filter(|v| !v.is_empty()) {
            Some(value) => out.push_str(value),
            None => out.push(UNKNOWN),
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    // Same normalisation as flow routing.
    let parts: Vec<Segment> = out
        .split(['/', '\\'])
        .filter(|p| !p.is_empty() && *p != "." && *p != "..")
        .map(|part| {
            if part.contains(UNKNOWN) {
                Segment::Pattern(part.split(UNKNOWN).map(str::to_string).collect())
            } else {
                Segment::Literal(part.to_string())
            }
        })
        .collect();
    // Any unsafe literal makes the whole template unusable for scanning.
    parts
        .iter()
        .all(|segment| match segment {
            Segment::Literal(part) => !part.contains(':'),
            Segment::Pattern(parts) => parts.iter().all(|p| !p.contains(':')),
        })
        .then_some(parts)
}

fn to_regex(parts: &[String]) -> Regex {
    let body: Vec<String> = parts.iter().map(|p| regex::escape(p)).collect();
    Regex::new(&format!("^{}$", body.join(".+"))).expect("escaped pattern is valid")
}

/// Folders for a destination, deduplicated so that no folder is nested in another.
pub(crate) fn existing_folders<'a>(
    store: &Store,
    resolver: &dyn RootResolver,
    space: &Space,
    destination: &Destination,
    catalog_paths: impl Iterator<Item = &'a str>,
) -> Result<Vec<String>, String> {
    let flows: Vec<Flow> = store
        .list_by("space_id", &space.id)
        .map_err(|e| e.to_string())?;
    let projects: Vec<Project> = store
        .list_by("space_id", &space.id)
        .map_err(|e| e.to_string())?;
    let mut templates: BTreeSet<Vec<Segment>> = BTreeSet::new();
    let project_independent = !uses_project_variables(
        &destination.path_template,
        space,
        &projects,
        destination.use_backup_marker,
    );
    let mut choices: Vec<Option<&Project>> = projects.iter().map(Some).collect();
    if project_independent {
        choices.push(None);
    }
    // Without a flow (removed or not yet configured), the source name is unknown.
    for project in &choices {
        let mut vars = project_template_vars(space, *project, &Device::default());
        vars.remove("source_name");
        if let Some(mut parts) = segments(&destination.path_template, &vars) {
            if destination.subfolder_per_source {
                parts.push(Segment::Pattern(vec![String::new(), String::new()]));
            }
            templates.insert(parts);
        }
    }
    for flow in flows.iter().filter(|f| f.destination_id == destination.id) {
        let Some(source) = store
            .get::<Source>(&flow.source_id)
            .map_err(|e| e.to_string())?
        else {
            continue;
        };
        let Some(device) = store
            .get::<Device>(&source.device_id)
            .map_err(|e| e.to_string())?
        else {
            continue;
        };
        let marker = destination
            .use_backup_marker
            .then(|| {
                resolver
                    .device_root(&source.device_id)
                    .filter(|root| root.exists())
                    .and_then(|root| read_backup_folder(&root))
            })
            .flatten();
        for project in &choices {
            let mut vars = source_template_vars(space, *project, &device, &source);
            let mut variants = vec![vars.clone()];
            if let Some(marker) = &marker {
                vars.insert("backup_folder".into(), marker.clone());
                variants.push(vars);
            }
            for vars in variants {
                let Some(mut parts) = segments(&destination.path_template, &vars) else {
                    continue;
                };
                if destination.subfolder_per_source {
                    let name = crate::plan::sanitize_segment(source.resolved_backup_name(&device));
                    parts.push(Segment::Literal(name));
                }
                templates.insert(parts);
            }
        }
    }
    let mut folders = BTreeSet::new();
    let mut patterns = Vec::new();
    for parts in templates {
        if parts.iter().all(|p| matches!(p, Segment::Literal(_))) {
            folders.insert(literal_path(&parts));
        } else {
            patterns.push(
                parts
                    .into_iter()
                    .map(|part| match part {
                        Segment::Literal(value) => Err(value),
                        Segment::Pattern(parts) => Ok(to_regex(&parts)),
                    })
                    .collect::<Vec<_>>(),
            );
        }
    }
    if !patterns.is_empty() {
        for path in catalog_paths {
            let segments: Vec<&str> = path.split('/').collect();
            for pattern in &patterns {
                // Files live below the folder, so the path must be strictly longer.
                if segments.len() > pattern.len()
                    && pattern
                        .iter()
                        .zip(&segments)
                        .all(|(part, segment)| match part {
                            Err(literal) => literal == segment,
                            Ok(regex) => regex.is_match(segment),
                        })
                {
                    folders.insert(segments[..pattern.len()].join("/"));
                }
            }
        }
    }
    let mut result: Vec<String> = Vec::new();
    for folder in folders {
        if !result
            .iter()
            .any(|parent| super::destination_check::under(&folder, parent))
        {
            result.push(folder);
        }
    }
    Ok(result)
}

fn literal_path(parts: &[Segment]) -> String {
    parts
        .iter()
        .map(|part| match part {
            Segment::Literal(value) => value.as_str(),
            Segment::Pattern(_) => unreachable!(),
        })
        .collect::<Vec<_>>()
        .join("/")
}
