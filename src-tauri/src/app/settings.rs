use crate::store::{Store, StoreResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const KEY: &str = "app_settings";

/// Per-computer preferences (not synced).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    /// `system`, `light` or `dark`.
    pub theme: String,
    pub auto_sync: bool,
    pub auto_sync_minutes: u32,
    pub sync_port: u16,
    pub active_space_id: Option<String>,
    pub active_project_by_space: HashMap<String, String>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: "system".into(),
            auto_sync: true,
            auto_sync_minutes: 5,
            sync_port: crate::sync::DEFAULT_PORT,
            active_space_id: None,
            active_project_by_space: HashMap::new(),
        }
    }
}

impl AppSettings {
    pub fn load(store: &Store) -> Self {
        store
            .setting(KEY)
            .ok()
            .flatten()
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, store: &Store) -> StoreResult<()> {
        store.set_setting(KEY, &serde_json::to_string(self).expect("settings serialize"))
    }
}
