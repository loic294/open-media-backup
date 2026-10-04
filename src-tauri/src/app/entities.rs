use crate::domain::{
    Computer, Destination, Device, DeviceMapping, Entity, EntityKind, Flow, Project, Source, Space,
};
use crate::store::Store;
use serde_json::Value;

type Result<T> = std::result::Result<T, String>;

fn put<E: Entity>(store: &Store, value: Value) -> Result<()> {
    let entity: E =
        serde_json::from_value(value).map_err(|e| format!("invalid {}: {e}", E::KIND.as_str()))?;
    if entity.id().is_empty() {
        return Err("entity id is required".into());
    }
    store.put(&entity).map_err(|e| e.to_string())
}

/// Saves a configuration entity coming from the UI. Catalog entities are engine-owned.
pub fn save_entity(store: &Store, kind: &str, value: Value) -> Result<()> {
    validate_save(store, kind, &value)?;
    match EntityKind::parse(kind).ok_or_else(|| format!("unknown kind {kind}"))? {
        EntityKind::Space => put::<Space>(store, value),
        EntityKind::Project => put::<Project>(store, value),
        EntityKind::Device => put::<Device>(store, value),
        EntityKind::DeviceMapping => put::<DeviceMapping>(store, value),
        EntityKind::Computer => put::<Computer>(store, value),
        EntityKind::Source => put::<Source>(store, value),
        EntityKind::Destination => put::<Destination>(store, value),
        EntityKind::Flow => put::<Flow>(store, value),
        EntityKind::FileRecord | EntityKind::FileCopy => {
            Err("file catalog entries are read-only".into())
        }
    }
}

fn validate_save(store: &Store, kind: &str, value: &Value) -> Result<()> {
    let err = |e: crate::store::StoreError| e.to_string();
    let mut spaces = store.list::<Space>().map_err(err)?;
    let mut projects = store.list::<Project>().map_err(err)?;
    let mut sources = store.list::<Source>().map_err(err)?;
    let mut destinations = store.list::<Destination>().map_err(err)?;
    let mut flows = store.list::<Flow>().map_err(err)?;
    fn replace<E: Entity>(entities: &mut Vec<E>, value: &Value) -> Result<()> {
        let entity: E = serde_json::from_value(value.clone())
            .map_err(|e| format!("invalid {}: {e}", E::KIND.as_str()))?;
        entities.retain(|e| e.id() != entity.id());
        entities.push(entity);
        Ok(())
    }
    match kind {
        "space" => replace(&mut spaces, value)?,
        "project" => {
            let project: Project = serde_json::from_value(value.clone())
                .map_err(|e| format!("invalid project: {e}"))?;
            project.capture_range()?;
            replace(&mut projects, value)?;
        }
        "source" => replace(&mut sources, value)?,
        "destination" => replace(&mut destinations, value)?,
        "flow" => replace(&mut flows, value)?,
        _ => return Ok(()),
    }
    // Only revalidate the changed entity's space, so unrelated synced configuration
    // errors do not prevent repairing another space.
    let space_id = if kind == "space" {
        value.get("id")
    } else {
        value.get("space_id")
    }
    .and_then(Value::as_str);
    let Some(space) = spaces.iter().find(|s| Some(s.id.as_str()) == space_id) else {
        return Ok(());
    };
    let own_projects: Vec<Project> = projects
        .into_iter()
        .filter(|p| p.space_id == space.id)
        .collect();
    crate::domain::validate_project_ranges(&own_projects, space.allow_project_overlap)?;
    for source in sources.iter().filter(|s| s.space_id == space.id) {
        source.validate_projects(&own_projects)?;
    }
    for flow in flows.iter().filter(|f| f.space_id == space.id) {
        if let (Some(source), Some(destination)) = (
            sources.iter().find(|s| s.id == flow.source_id),
            destinations.iter().find(|d| d.id == flow.destination_id),
        ) {
            crate::plan::validate_source_destination(source, destination, space, &own_projects)?;
        }
    }
    Ok(())
}

/// Deletes an entity and everything that depends on it.
pub fn delete_entity(store: &Store, kind: &str, id: &str) -> Result<()> {
    let kind = EntityKind::parse(kind).ok_or_else(|| format!("unknown kind {kind}"))?;
    let err = |e: crate::store::StoreError| e.to_string();
    let ids = |list: Vec<String>, kind: EntityKind| -> Result<()> {
        list.iter()
            .try_for_each(|id| store.delete(kind, id).map_err(err))
    };
    match kind {
        EntityKind::Space => {
            ids(
                store
                    .list_by::<Flow>("space_id", id)
                    .map_err(err)?
                    .into_iter()
                    .map(|e| e.id)
                    .collect(),
                EntityKind::Flow,
            )?;
            ids(
                store
                    .list_by::<Source>("space_id", id)
                    .map_err(err)?
                    .into_iter()
                    .map(|e| e.id)
                    .collect(),
                EntityKind::Source,
            )?;
            ids(
                store
                    .list_by::<Destination>("space_id", id)
                    .map_err(err)?
                    .into_iter()
                    .map(|e| e.id)
                    .collect(),
                EntityKind::Destination,
            )?;
            ids(
                store
                    .list_by::<Project>("space_id", id)
                    .map_err(err)?
                    .into_iter()
                    .map(|e| e.id)
                    .collect(),
                EntityKind::Project,
            )?;
        }
        EntityKind::Source => {
            ids(
                store
                    .list_by::<Flow>("source_id", id)
                    .map_err(err)?
                    .into_iter()
                    .map(|e| e.id)
                    .collect(),
                EntityKind::Flow,
            )?;
        }
        EntityKind::Destination => {
            ids(
                store
                    .list_by::<Flow>("destination_id", id)
                    .map_err(err)?
                    .into_iter()
                    .map(|e| e.id)
                    .collect(),
                EntityKind::Flow,
            )?;
        }
        EntityKind::Project => {
            for mut source in store.list::<Source>().map_err(err)? {
                if let crate::domain::ProjectScope::Selected { project_ids } =
                    &mut source.project_scope
                {
                    let count = project_ids.len();
                    project_ids.retain(|project_id| project_id != id);
                    if project_ids.len() != count {
                        store.put(&source).map_err(err)?;
                    }
                }
            }
        }
        EntityKind::Device => {
            return store.delete_device(id).map_err(err);
        }
        EntityKind::FileRecord | EntityKind::FileCopy => {
            return Err("file catalog entries are read-only".into())
        }
        _ => {}
    }
    store.delete(kind, id).map_err(err)
}
