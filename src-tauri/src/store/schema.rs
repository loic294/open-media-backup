use rusqlite::Connection;

const MIGRATIONS: &[&str] = &[r#"
CREATE TABLE entities (
  kind TEXT NOT NULL,
  id TEXT NOT NULL,
  data TEXT NOT NULL,
  deleted INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (kind, id)
);
CREATE TABLE field_clocks (
  kind TEXT NOT NULL,
  id TEXT NOT NULL,
  field TEXT NOT NULL,
  hlc TEXT NOT NULL,
  PRIMARY KEY (kind, id, field)
);
CREATE TABLE oplog (
  hlc TEXT PRIMARY KEY,
  origin TEXT NOT NULL,
  kind TEXT NOT NULL,
  entity_id TEXT NOT NULL,
  field TEXT NOT NULL,
  value TEXT NOT NULL
);
CREATE INDEX oplog_origin ON oplog (origin, hlc);
CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE peers (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  address TEXT NOT NULL,
  token TEXT NOT NULL,
  last_seen INTEGER,
  last_error TEXT
);
"#];

pub fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    let version: usize = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    for (index, sql) in MIGRATIONS.iter().enumerate().skip(version) {
        conn.execute_batch(sql)?;
        conn.pragma_update(None, "user_version", index + 1)?;
    }
    Ok(())
}
