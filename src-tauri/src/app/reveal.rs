use crate::domain::{
    Destination, DestinationKind, Device, Flow, Project, ProjectScope, Source, Space,
};
use crate::paths::{expand, join_relative};
use crate::plan::{backup_folder_name, project_template_vars, resolve_flow, RootResolver};
use crate::store::Store;
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RevealKind {
    Source,
    Destination,
}

pub fn reveal_space_id(store: &Store, kind: RevealKind, id: &str) -> Result<String, String> {
    match kind {
        RevealKind::Source => load::<Source>(store, id, "source").map(|source| source.space_id),
        RevealKind::Destination => {
            load::<Destination>(store, id, "destination").map(|destination| destination.space_id)
        }
    }
}

pub fn resolve_reveal_path(
    store: &Store,
    resolver: &dyn RootResolver,
    project_id: Option<&str>,
    kind: RevealKind,
    id: &str,
) -> Result<PathBuf, String> {
    if let Some(path) = resolve_via_flow(store, resolver, project_id, kind, id)? {
        return Ok(path);
    }
    match kind {
        RevealKind::Source => resolve_source_path(store, resolver, project_id, id),
        RevealKind::Destination => resolve_destination_path(store, resolver, project_id, id),
    }
}

fn resolve_via_flow(
    store: &Store,
    resolver: &dyn RootResolver,
    project_id: Option<&str>,
    kind: RevealKind,
    id: &str,
) -> Result<Option<PathBuf>, String> {
    let Some(project_id) = project_id else {
        return Ok(None);
    };
    let space_id = reveal_space_id(store, kind, id)?;
    let mut flows: Vec<Flow> = store
        .list_by("space_id", &space_id)
        .map_err(|e| e.to_string())?;
    flows.sort_by(|a, b| a.id.cmp(&b.id));
    for flow in flows {
        let matches = match kind {
            RevealKind::Source => flow.source_id == id,
            RevealKind::Destination => flow.destination_id == id,
        };
        if !matches {
            continue;
        }
        let ctx = resolve_flow(store, resolver, project_id, &flow.id).map_err(|e| e.to_string())?;
        return match kind {
            RevealKind::Source => ctx.source_folder().map(Some).ok_or_else(|| {
                format!(
                    "{} is not connected on this computer",
                    ctx.source_device.name
                )
            }),
            RevealKind::Destination => {
                if ctx.destination.kind == DestinationKind::App {
                    Err("App destinations do not have a folder to reveal".into())
                } else {
                    ctx.dest_root
                        .as_ref()
                        .map(|root| join_relative(root, &ctx.dest_folder_rel))
                        .map(Some)
                        .ok_or_else(|| {
                            format!("{} is not connected on this computer", ctx.dest_device.name)
                        })
                }
            }
        };
    }
    Ok(None)
}

fn resolve_source_path(
    store: &Store,
    resolver: &dyn RootResolver,
    project_id: Option<&str>,
    source_id: &str,
) -> Result<PathBuf, String> {
    let source: Source = load(store, source_id, "source")?;
    let space: Space = load(store, &source.space_id, "space")?;
    let device: Device = load(store, &source.device_id, "device")?;
    let project = load_project(store, project_id, &space.id)?;
    let root = resolver
        .device_root(&device.id)
        .filter(|path| path.exists())
        .ok_or_else(|| format!("{} is not connected on this computer", device.name))?;
    let vars = if matches!(source.project_scope, ProjectScope::None) {
        project_template_vars(&space, None, &device)
    } else {
        project_template_vars(&space, project.as_ref(), &device)
    };
    let folder = normalize(&expand(&source.path_template, &vars).map_err(|e| e.to_string())?);
    Ok(join_relative(&root, &folder))
}

fn resolve_destination_path(
    store: &Store,
    resolver: &dyn RootResolver,
    project_id: Option<&str>,
    destination_id: &str,
) -> Result<PathBuf, String> {
    let destination: Destination = load(store, destination_id, "destination")?;
    if destination.kind == DestinationKind::App {
        return Err("App destinations do not have a folder to reveal".into());
    }
    let space: Space = load(store, &destination.space_id, "space")?;
    let device: Device = load(store, &destination.device_id, "device")?;
    let project = load_project(store, project_id, &space.id)?;
    let root = resolver
        .device_root(&device.id)
        .filter(|path| path.exists())
        .ok_or_else(|| format!("{} is not connected on this computer", device.name))?;
    let mut vars = project_template_vars(&space, project.as_ref(), &Device::default());
    if destination.use_backup_marker {
        let backup_folder = backup_folder_name(&space, &vars, None)?;
        vars.insert("backup_folder".into(), backup_folder);
    }
    let folder = normalize(&expand(&destination.path_template, &vars).map_err(|e| e.to_string())?);
    Ok(join_relative(&root, &folder))
}

fn load<E: crate::domain::Entity>(store: &Store, id: &str, what: &str) -> Result<E, String> {
    store
        .get::<E>(id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("{what} {id} not found"))
}

fn load_project(
    store: &Store,
    project_id: Option<&str>,
    space_id: &str,
) -> Result<Option<Project>, String> {
    let Some(project_id) = project_id else {
        return Ok(None);
    };
    let project: Project = load(store, project_id, "project")?;
    if project.space_id != space_id {
        return Err("Project belongs to a different space".into());
    }
    Ok(Some(project))
}

fn normalize(path: &str) -> String {
    path.split(['/', '\\'])
        .filter(|p| !p.is_empty() && *p != "." && *p != "..")
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::DestinationKind;
    use crate::testing::Fixture;

    #[test]
    fn source_resolves_to_mounted_source_folder() {
        let fx = Fixture::new();
        let path = resolve_reveal_path(
            &fx.store,
            &fx.resolver,
            Some(&fx.project.id),
            RevealKind::Source,
            &fx.source.id,
        )
        .unwrap();
        assert_eq!(path, fx.card_dir.path().join("DCIM"));
    }

    #[test]
    fn destination_resolves_like_flow_destination_folder() {
        let fx = Fixture::new();
        let path = resolve_reveal_path(
            &fx.store,
            &fx.resolver,
            Some(&fx.project.id),
            RevealKind::Destination,
            &fx.destination.id,
        )
        .unwrap();
        assert_eq!(
            path,
            fx.nas_dir
                .path()
                .join("photo")
                .join("Trip")
                .join("Camera A Card 1")
        );
    }

    #[test]
    fn app_destination_errors() {
        let fx = Fixture::new();
        let mut destination = fx.destination.clone();
        destination.kind = DestinationKind::App;
        fx.store.put(&destination).unwrap();
        let error = resolve_reveal_path(
            &fx.store,
            &fx.resolver,
            Some(&fx.project.id),
            RevealKind::Destination,
            &destination.id,
        )
        .unwrap_err();
        assert!(error.contains("App destinations do not have a folder"));
    }

    #[test]
    fn unmounted_source_errors() {
        let fx = Fixture::new();
        fx.unmount(&fx.card.id);
        let error = resolve_reveal_path(
            &fx.store,
            &fx.resolver,
            Some(&fx.project.id),
            RevealKind::Source,
            &fx.source.id,
        )
        .unwrap_err();
        assert!(error.contains("Camera A Card 1 is not connected"));
    }

    #[test]
    fn unmounted_destination_errors() {
        let fx = Fixture::new();
        fx.unmount(&fx.nas.id);
        let error = resolve_reveal_path(
            &fx.store,
            &fx.resolver,
            Some(&fx.project.id),
            RevealKind::Destination,
            &fx.destination.id,
        )
        .unwrap_err();
        assert!(error.contains("Home NAS is not connected"));
    }
}
