use crate::store::{Store, StoreResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const KEY: &str = "app_settings";
const MIN_LEARNED_BYTES: u64 = 50 * 1024 * 1024;
const MIN_LEARNED_SECS: f64 = 2.0;
const TRANSFER_SPEED_ALPHA: f64 = 0.25;
pub const GLOBAL_TRANSFER_SPEED_KEY: &str = "_global";

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
    pub preview_apps: PreviewAppSettings,
    pub app_destinations: HashMap<String, String>,
    pub transfer_speeds: HashMap<String, u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PreviewAppSettings {
    pub photos: Option<String>,
    pub videos: Option<String>,
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
            preview_apps: PreviewAppSettings::default(),
            app_destinations: HashMap::new(),
            transfer_speeds: HashMap::new(),
        }
    }
}

impl AppSettings {
    pub fn learned_transfer_speed(&self, destination_device_id: &str) -> Option<u64> {
        self.transfer_speeds
            .get(destination_device_id)
            .copied()
            .or_else(|| self.transfer_speeds.get(GLOBAL_TRANSFER_SPEED_KEY).copied())
            .filter(|speed| *speed > 0)
    }

    pub fn record_transfer_sample(
        &mut self,
        destination_device_id: &str,
        bytes: u64,
        elapsed_secs: f64,
    ) -> bool {
        if bytes < MIN_LEARNED_BYTES || elapsed_secs < MIN_LEARNED_SECS {
            return false;
        }
        let sample = (bytes as f64 / elapsed_secs).round() as u64;
        if sample == 0 {
            return false;
        }
        update_speed(&mut self.transfer_speeds, destination_device_id, sample);
        update_speed(&mut self.transfer_speeds, GLOBAL_TRANSFER_SPEED_KEY, sample);
        true
    }

    pub fn load(store: &Store) -> Self {
        store
            .setting(KEY)
            .ok()
            .flatten()
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, store: &Store) -> StoreResult<()> {
        store.set_setting(
            KEY,
            &serde_json::to_string(self).expect("settings serialize"),
        )
    }
}

fn update_speed(speeds: &mut HashMap<String, u64>, key: &str, sample: u64) {
    let value = match speeds.get(key).copied() {
        Some(current) if current > 0 => ((current as f64 * (1.0 - TRANSFER_SPEED_ALPHA))
            + (sample as f64 * TRANSFER_SPEED_ALPHA))
            .round() as u64,
        _ => sample,
    };
    speeds.insert(key.to_string(), value.max(1));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn learned_transfer_speed_uses_device_then_global() {
        let mut settings = AppSettings::default();
        settings
            .transfer_speeds
            .insert(GLOBAL_TRANSFER_SPEED_KEY.into(), 100);
        assert_eq!(settings.learned_transfer_speed("nas"), Some(100));
        settings.transfer_speeds.insert("nas".into(), 200);
        assert_eq!(settings.learned_transfer_speed("nas"), Some(200));
    }

    #[test]
    fn record_transfer_sample_updates_device_and_global_as_ewma() {
        let mut settings = AppSettings::default();
        assert!(settings.record_transfer_sample("nas", 100 * 1024 * 1024, 2.0));
        assert_eq!(settings.transfer_speeds["nas"], 52_428_800);
        assert!(settings.record_transfer_sample("nas", 100 * 1024 * 1024, 4.0));
        assert_eq!(settings.transfer_speeds["nas"], 45_875_200);
        assert_eq!(
            settings.transfer_speeds[GLOBAL_TRANSFER_SPEED_KEY],
            45_875_200
        );
    }

    #[test]
    fn record_transfer_sample_ignores_tiny_jobs() {
        let mut settings = AppSettings::default();
        assert!(!settings.record_transfer_sample("nas", 49 * 1024 * 1024, 10.0));
        assert!(!settings.record_transfer_sample("nas", 100 * 1024 * 1024, 1.99));
        assert!(settings.transfer_speeds.is_empty());
    }

    #[test]
    fn learned_transfer_speeds_persist_in_app_settings() {
        let fx = crate::testing::Fixture::new();
        let mut settings = AppSettings::default();
        assert!(settings.record_transfer_sample("nas", 100 * 1024 * 1024, 2.0));
        settings.save(&fx.store).unwrap();
        let loaded = AppSettings::load(&fx.store);
        assert_eq!(loaded.learned_transfer_speed("nas"), Some(52_428_800));
        assert_eq!(loaded.learned_transfer_speed("ssd"), Some(52_428_800));
    }
}
