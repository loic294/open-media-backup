use super::{
    apply::apply_op, ops::DELETED_FIELD, Op, Store, StoreError, StoreResult, VersionVector,
};
use crate::domain::{Entity, EntityKind};
use rusqlite::{params, OptionalExtension, Transaction};
use serde_json::{Map, Value};

impl Store {
    pub fn get<E: Entity>(&self, id: &str) -> StoreResult<Option<E>> {
        let conn = self.conn.lock();
        let data: Option<String> = conn
            .query_row(
                "SELECT data FROM entities WHERE kind = ?1 AND id = ?2 AND deleted = 0",
                params![E::KIND.as_str(), id],
                |r| r.get(0),
            )
            .optional()?;
        data.map(|d| serde_json::from_str(&d).map_err(Into::into))
            .transpose()
    }

    pub fn list<E: Entity>(&self) -> StoreResult<Vec<E>> {
        self.query_docs(
            "SELECT data FROM entities WHERE kind = ?1 AND deleted = 0",
            params![E::KIND.as_str()],
        )
    }

    /// Lists entities whose top-level JSON `field` equals `value`.
    pub fn list_by<E: Entity>(&self, field: &str, value: &str) -> StoreResult<Vec<E>> {
        let path = format!("$.{field}");
        self.query_docs(
            "SELECT data FROM entities WHERE kind = ?1 AND deleted = 0 AND json_extract(data, ?2) = ?3",
            params![E::KIND.as_str(), path, value],
        )
    }

    fn query_docs<E: Entity>(&self, sql: &str, args: impl rusqlite::Params) -> StoreResult<Vec<E>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map(args, |r| r.get::<_, String>(0))?;
        rows.map(|row| Ok(serde_json::from_str(&row?)?)).collect()
    }

    pub fn put<E: Entity>(&self, entity: &E) -> StoreResult<()> {
        self.put_all(std::slice::from_ref(entity))
    }

    /// Saves entities, emitting one op per changed top-level field.
    pub fn put_all<E: Entity>(&self, entities: &[E]) -> StoreResult<()> {
        let mut changed = false;
        {
            let mut conn = self.conn.lock();
            let tx = conn.transaction()?;
            for entity in entities {
                if entity.id().is_empty() {
                    return Err(StoreError::Invalid(format!(
                        "{} without id",
                        E::KIND.as_str()
                    )));
                }
                for op in self.diff_ops(&tx, E::KIND, entity.id(), serde_json::to_value(entity)?)? {
                    changed |= apply_op(&tx, &op)?;
                }
            }
            tx.commit()?;
        }
        if changed {
            self.notify(vec![E::KIND.as_str().to_string()], false);
        }
        Ok(())
    }

    pub fn delete(&self, kind: EntityKind, id: &str) -> StoreResult<()> {
        let op = self.local_op(kind, id, DELETED_FIELD, Value::Bool(true));
        let changed = {
            let mut conn = self.conn.lock();
            let tx = conn.transaction()?;
            let changed = apply_op(&tx, &op)?;
            tx.commit()?;
            changed
        };
        if changed {
            self.notify(vec![kind.as_str().to_string()], false);
        }
        Ok(())
    }

    /// Detaches tasks and forgets a device in one synced transaction.
    pub fn delete_device(&self, id: &str) -> StoreResult<()> {
        use crate::domain::{Destination, DeviceMapping, SafeCopyOverride, Source};
        if id.is_empty() {
            return Err(StoreError::Invalid("device id is required".into()));
        }
        let mut kinds = Vec::new();
        {
            let mut conn = self.conn.lock();
            let tx = conn.transaction()?;
            for kind in [Source::KIND, Destination::KIND, DeviceMapping::KIND] {
                let ids: Vec<String> = {
                    let mut stmt = tx.prepare(
                        "SELECT id FROM entities WHERE kind = ?1 AND deleted = 0 AND json_extract(data, '$.device_id') = ?2",
                    )?;
                    let rows = stmt.query_map(params![kind.as_str(), id], |r| r.get(0))?;
                    rows.collect::<Result<_, _>>()?
                };
                for entity_id in ids {
                    let (field, value) = if kind == DeviceMapping::KIND {
                        (DELETED_FIELD, Value::Bool(true))
                    } else {
                        ("device_id", Value::String(String::new()))
                    };
                    if apply_op(&tx, &self.local_op(kind, &entity_id, field, value))?
                        && !kinds.contains(&kind.as_str().to_string())
                    {
                        kinds.push(kind.as_str().to_string());
                    }
                    let override_ids: Vec<String> = {
                        let mut stmt = tx.prepare(
                            "SELECT id FROM entities WHERE kind = ?1 AND deleted = 0 AND (json_extract(data, '$.source_device_id') = ?2 OR json_extract(data, '$.destination_device_id') = ?2)",
                        )?;
                        let rows = stmt
                            .query_map(params![SafeCopyOverride::KIND.as_str(), id], |r| {
                                r.get(0)
                            })?;
                        rows.collect::<Result<_, _>>()?
                    };
                    for override_id in override_ids {
                        if apply_op(
                            &tx,
                            &self.local_op(
                                SafeCopyOverride::KIND,
                                &override_id,
                                DELETED_FIELD,
                                Value::Bool(true),
                            ),
                        )? {
                            kinds.push(SafeCopyOverride::KIND.as_str().to_string());
                        }
                    }
                }
            }
            if apply_op(
                &tx,
                &self.local_op(EntityKind::Device, id, DELETED_FIELD, Value::Bool(true)),
            )? {
                kinds.push(EntityKind::Device.as_str().to_string());
            }
            tx.commit()?;
        }
        self.notify(kinds, false);
        Ok(())
    }

    fn diff_ops(
        &self,
        tx: &Transaction,
        kind: EntityKind,
        id: &str,
        value: Value,
    ) -> StoreResult<Vec<Op>> {
        let Value::Object(fields) = value else {
            return Err(StoreError::Invalid(
                "entity must serialize to an object".into(),
            ));
        };
        let existing: Option<(String, bool)> = tx
            .query_row(
                "SELECT data, deleted FROM entities WHERE kind = ?1 AND id = ?2",
                params![kind.as_str(), id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let (old, deleted) = match existing {
            Some((data, deleted)) => (serde_json::from_str::<Map<String, Value>>(&data)?, deleted),
            None => (Map::new(), false),
        };
        let mut ops: Vec<Op> = fields
            .into_iter()
            .filter(|(field, val)| field != "id" && old.get(field) != Some(val))
            .map(|(field, val)| self.local_op(kind, id, &field, val))
            .collect();
        if deleted {
            ops.push(self.local_op(kind, id, DELETED_FIELD, Value::Bool(false)));
        }
        Ok(ops)
    }

    fn local_op(&self, kind: EntityKind, id: &str, field: &str, value: Value) -> Op {
        Op {
            hlc: self.clock.now().encode(),
            origin: self.computer_id.clone(),
            kind: kind.as_str().to_string(),
            entity_id: id.to_string(),
            field: field.to_string(),
            value,
        }
    }

    pub fn version_vector(&self) -> StoreResult<VersionVector> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT origin, MAX(hlc) FROM oplog GROUP BY origin")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
}
