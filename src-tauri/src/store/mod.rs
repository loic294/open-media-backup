//! SQLite-backed document store. Every synced change is recorded as an [`Op`]
//! stamped with a hybrid logical clock and merged last-writer-wins per field.
pub mod analysis;
mod apply;
mod clock;
mod entities;
mod error;
mod local;
mod ops;
mod schema;

pub use clock::{Hlc, HlcClock};
pub use error::{StoreError, StoreResult};
pub use local::{HashServer, Peer};
pub use ops::{Op, VersionVector};

use parking_lot::Mutex;
use rusqlite::Connection;
use std::path::Path;
use tokio::sync::broadcast;

/// Emitted whenever entities change (locally or from a peer).
#[derive(Debug, Clone)]
pub struct ChangeEvent {
    pub kinds: Vec<String>,
    pub remote: bool,
}

pub struct Store {
    conn: Mutex<Connection>,
    clock: HlcClock,
    computer_id: String,
    changes: broadcast::Sender<ChangeEvent>,
    analysis_recovery_error: Option<String>,
}

impl Store {
    pub fn open(path: &Path) -> StoreResult<Self> {
        Self::from_connection(Connection::open(path)?)
    }

    pub fn open_in_memory() -> StoreResult<Self> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(conn: Connection) -> StoreResult<Self> {
        schema::migrate(&conn)?;
        let analysis_recovery_error = analysis::recover_interrupted(&conn).err().map(|error| {
            let message = format!("Could not recover interrupted speed analysis: {error}");
            log::error!("{message}");
            message
        });
        let computer_id = local::computer_id(&conn)?;
        let (changes, _) = broadcast::channel(64);
        let store = Self {
            clock: HlcClock::new(computer_id.clone()),
            conn: Mutex::new(conn),
            computer_id,
            changes,
            analysis_recovery_error,
        };
        store.migrate_copy_policy()?;
        Ok(store)
    }

    pub fn computer_id(&self) -> &str {
        &self.computer_id
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ChangeEvent> {
        self.changes.subscribe()
    }

    fn notify(&self, kinds: Vec<String>, remote: bool) {
        if !kinds.is_empty() {
            let _ = self.changes.send(ChangeEvent { kinds, remote });
        }
    }
}

#[cfg(test)]
mod tests;
