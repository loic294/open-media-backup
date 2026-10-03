use super::AppSettings;
use crate::domain::{Computer, Destination, Device, DeviceMapping, Flow, Project, Source, Space};
use crate::store::{Store, StoreResult};
use serde::Serialize;

/// Every configuration entity the UI needs (file records are queried on demand).
#[derive(Debug, Clone, Serialize)]
pub struct Snapshot {
    pub computer: Computer,
    pub computers: Vec<Computer>,
    pub spaces: Vec<Space>,
    pub projects: Vec<Project>,
    pub devices: Vec<Device>,
    pub mappings: Vec<DeviceMapping>,
    pub sources: Vec<Source>,
    pub destinations: Vec<Destination>,
    pub flows: Vec<Flow>,
    pub settings: AppSettings,
}

impl Snapshot {
    pub fn load(store: &Store) -> StoreResult<Self> {
        let computers: Vec<Computer> = store.list()?;
        let computer = computers
            .iter()
            .find(|c| c.id == store.computer_id())
            .cloned()
            .unwrap_or_else(|| local_computer(store));
        let mut spaces: Vec<Space> = store.list()?;
        spaces.sort_by_key(|s| s.position);
        Ok(Self {
            computer,
            computers,
            spaces,
            projects: store.list()?,
            devices: store.list()?,
            mappings: store.list()?,
            sources: store.list()?,
            destinations: store.list()?,
            flows: store.list()?,
            settings: AppSettings::load(store),
        })
    }
}

pub fn local_computer(store: &Store) -> Computer {
    Computer {
        id: store.computer_id().to_string(),
        name: gethostname::gethostname()
            .to_string_lossy()
            .trim_end_matches(".local")
            .to_string(),
        os: std::env::consts::OS.to_string(),
    }
}
