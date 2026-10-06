use super::{apply::apply_op, Hlc, Op, Store, StoreResult, VersionVector};
use crate::domain::EntityKind;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

/// A peer computer reachable over the network (local-only, never synced).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Peer {
    pub id: String,
    pub name: String,
    pub address: String,
    pub token: String,
    pub last_seen: Option<i64>,
    pub last_error: Option<String>,
}

/// Private pairing information for a read-only hash server; never an entity or sync op.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct HashServer {
    pub id: String,
    pub name: String,
    pub address: String,
    #[serde(skip_serializing)]
    pub token: String,
    pub last_seen: Option<i64>,
    pub last_error: Option<String>,
}

pub(super) fn computer_id(conn: &Connection) -> rusqlite::Result<String> {
    let existing: Option<String> = conn
        .query_row(
            "SELECT value FROM settings WHERE key = 'computer_id'",
            [],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(id) = existing {
        return Ok(id);
    }
    let id = crate::domain::new_id();
    conn.execute(
        "INSERT INTO settings (key, value) VALUES ('computer_id', ?1)",
        [&id],
    )?;
    Ok(id)
}

impl Store {
    pub fn hash_servers(&self) -> StoreResult<Vec<HashServer>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT id, name, address, token, last_seen, last_error FROM hash_servers ORDER BY name")?;
        let rows = stmt.query_map([], |r| {
            Ok(HashServer {
                id: r.get(0)?,
                name: r.get(1)?,
                address: r.get(2)?,
                token: r.get(3)?,
                last_seen: r.get(4)?,
                last_error: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn save_hash_server(&self, server: &HashServer) -> StoreResult<()> {
        self.conn.lock().execute(
            "INSERT OR REPLACE INTO hash_servers (id, name, address, token, last_seen, last_error) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![server.id, server.name, server.address, server.token, server.last_seen, server.last_error],
        )?;
        Ok(())
    }

    pub fn remove_hash_server(&self, id: &str) -> StoreResult<()> {
        self.conn
            .lock()
            .execute("DELETE FROM hash_servers WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn setting(&self, key: &str) -> StoreResult<Option<String>> {
        let conn = self.conn.lock();
        Ok(conn
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
                r.get(0)
            })
            .optional()?)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> StoreResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
            [key, value],
        )?;
        Ok(())
    }

    pub fn peers(&self) -> StoreResult<Vec<Peer>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, name, address, token, last_seen, last_error FROM peers ORDER BY name",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(Peer {
                id: r.get(0)?,
                name: r.get(1)?,
                address: r.get(2)?,
                token: r.get(3)?,
                last_seen: r.get(4)?,
                last_error: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn save_peer(&self, peer: &Peer) -> StoreResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT OR REPLACE INTO peers (id, name, address, token, last_seen, last_error) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![peer.id, peer.name, peer.address, peer.token, peer.last_seen, peer.last_error],
        )?;
        Ok(())
    }

    pub fn remove_peer(&self, id: &str) -> StoreResult<()> {
        self.conn
            .lock()
            .execute("DELETE FROM peers WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Ops the holder of `known` has not seen yet, in causal order.
    pub fn ops_since(&self, known: &VersionVector, limit: usize) -> StoreResult<Vec<Op>> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare("SELECT hlc, origin, kind, entity_id, field, value FROM oplog ORDER BY hlc")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get::<_, String>(5)?,
            ))
        })?;
        let mut ops = Vec::new();
        for row in rows {
            let (hlc, origin, kind, entity_id, field, value) = row?;
            if known.get(&origin).is_some_and(|seen| *seen >= hlc) {
                continue;
            }
            ops.push(Op {
                hlc,
                origin,
                kind,
                entity_id,
                field,
                value: serde_json::from_str(&value)?,
            });
            if ops.len() >= limit {
                break;
            }
        }
        Ok(ops)
    }

    /// Merges ops received from a peer. Returns how many changed local state.
    pub fn apply_remote(&self, ops: &[Op]) -> StoreResult<usize> {
        let mut changed = 0;
        let mut kinds = Vec::new();
        {
            let mut conn = self.conn.lock();
            let tx = conn.transaction()?;
            for op in ops {
                if EntityKind::parse(&op.kind).is_none() {
                    continue;
                }
                if let Some(hlc) = Hlc::decode(&op.hlc) {
                    self.clock.observe(&hlc);
                }
                if apply_op(&tx, op)? {
                    changed += 1;
                    if !kinds.contains(&op.kind) {
                        kinds.push(op.kind.clone());
                    }
                }
            }
            tx.commit()?;
        }
        self.notify(kinds, true);
        Ok(changed)
    }
}
