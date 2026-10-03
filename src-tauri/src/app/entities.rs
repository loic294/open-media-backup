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
        EntityKind::Device => {
            let users = store.list_by::<Source>("device_id", id).map_err(err)?.len()
                + store
                    .list_by::<Destination>("device_id", id)
                    .map_err(err)?
                    .len();
            if users > 0 {
                return Err(format!(
                    "this device is still used by {users} source(s)/destination(s)"
                ));
            }
            ids(
                store
                    .list_by::<DeviceMapping>("device_id", id)
                    .map_err(err)?
                    .into_iter()
                    .map(|e| e.id)
                    .collect(),
                EntityKind::DeviceMapping,
            )?;
        }
        EntityKind::FileRecord | EntityKind::FileCopy => {
            return Err("file catalog entries are read-only".into())
        }
        _ => {}
    }
    store.delete(kind, id).map_err(err)
}
