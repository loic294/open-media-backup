use crate::domain::{FileCopy, FileRecord};
use crate::store::{Store, StoreResult};
use std::collections::{HashMap, HashSet};

/// In-memory index of known file copies, built once per planning pass.
#[derive(Default)]
pub struct Catalog {
    records: HashMap<String, FileRecord>,
    by_location: HashMap<(String, String), FileCopy>,
    devices_by_file: HashMap<String, HashSet<String>>,
}

impl Catalog {
    pub fn load(store: &Store) -> StoreResult<Self> {
        Ok(Self::from_parts(store.list::<FileRecord>()?, store.list::<FileCopy>()?))
    }

    pub fn from_parts(records: Vec<FileRecord>, copies: Vec<FileCopy>) -> Self {
        let mut catalog = Self { records: records.into_iter().map(|r| (r.id.clone(), r)).collect(), ..Default::default() };
        for copy in copies.into_iter().filter(|c| !c.removed) {
            catalog.devices_by_file.entry(copy.file_id.clone()).or_default().insert(copy.device_id.clone());
            catalog.by_location.insert((copy.device_id.clone(), copy.path.clone()), copy);
        }
        catalog
    }

    pub fn record(&self, file_id: &str) -> Option<&FileRecord> {
        self.records.get(file_id)
    }

    /// The known file at a device location, if its recorded size still matches.
    pub fn file_at(&self, device_id: &str, path: &str, size: Option<u64>) -> Option<&FileRecord> {
        let copy = self.by_location.get(&(device_id.to_string(), path.to_string()))?;
        let record = self.records.get(&copy.file_id)?;
        size.is_none_or(|s| s == record.size).then_some(record)
    }

    pub fn has_copy_on(&self, file_id: &str, device_id: &str) -> bool {
        self.devices_by_file.get(file_id).is_some_and(|d| d.contains(device_id))
    }

    /// Known copies on a device whose path is below `folder` (`""` = whole device).
    pub fn copies_under<'a>(&'a self, device_id: &'a str, folder: &'a str) -> impl Iterator<Item = &'a FileCopy> + 'a {
        let prefix = if folder.is_empty() { String::new() } else { format!("{}/", folder.trim_end_matches('/')) };
        self.by_location.values().filter(move |c| c.device_id == device_id && c.path.starts_with(&prefix))
    }
}
