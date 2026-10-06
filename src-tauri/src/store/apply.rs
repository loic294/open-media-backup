use super::{ops::DELETED_FIELD, Op, StoreResult};
use rusqlite::{params, OptionalExtension, Transaction};
use serde_json::{Map, Value};

/// Applies an op inside a transaction. Returns true when the entity changed.
pub(super) fn apply_op(tx: &Transaction, op: &Op) -> StoreResult<bool> {
    if op.kind == "space"
        && op.field == "final_copies_required"
        && op
            .value
            .as_u64()
            .is_none_or(|value| value > u32::MAX as u64)
    {
        return Err(super::StoreError::Invalid(
            "Required copies must be a non-negative 32-bit integer".into(),
        ));
    }
    let encoded = serde_json::to_string(&op.value)?;
    let inserted = tx.execute(
        "INSERT OR IGNORE INTO oplog (hlc, origin, kind, entity_id, field, value) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![op.hlc, op.origin, op.kind, op.entity_id, op.field, encoded],
    )?;
    if inserted == 0 {
        return Ok(false);
    }
    let current: Option<String> = tx
        .query_row(
            "SELECT hlc FROM field_clocks WHERE kind = ?1 AND id = ?2 AND field = ?3",
            params![op.kind, op.entity_id, op.field],
            |r| r.get(0),
        )
        .optional()?;
    if current.is_some_and(|hlc| hlc >= op.hlc) {
        return Ok(false);
    }
    tx.execute(
        "INSERT OR REPLACE INTO field_clocks (kind, id, field, hlc) VALUES (?1, ?2, ?3, ?4)",
        params![op.kind, op.entity_id, op.field, op.hlc],
    )?;
    write_field(tx, op)?;
    Ok(true)
}

fn write_field(tx: &Transaction, op: &Op) -> StoreResult<()> {
    let existing: Option<(String, bool)> = tx
        .query_row(
            "SELECT data, deleted FROM entities WHERE kind = ?1 AND id = ?2",
            params![op.kind, op.entity_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let (mut doc, mut deleted) = match existing {
        Some((data, deleted)) => (serde_json::from_str::<Map<String, Value>>(&data)?, deleted),
        None => (Map::new(), false),
    };
    doc.insert("id".into(), Value::String(op.entity_id.clone()));
    if op.field == DELETED_FIELD {
        deleted = op.value.as_bool().unwrap_or(false);
    } else {
        doc.insert(op.field.clone(), op.value.clone());
    }
    tx.execute(
        "INSERT OR REPLACE INTO entities (kind, id, data, deleted) VALUES (?1, ?2, ?3, ?4)",
        params![op.kind, op.entity_id, serde_json::to_string(&doc)?, deleted],
    )?;
    Ok(())
}
