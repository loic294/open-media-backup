use super::{identify::identify_volume, marker::read_marker, ProbeInfo};
use crate::{
    domain::{Device, DeviceMapping},
    store::Store,
};
use serde::Serialize;
use std::path::Path;
use sysinfo::Disks;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct VolumeInfo {
    pub mount_path: String,
    pub name: String,
    pub volume_uuid: Option<String>,
    pub hw_serial: Option<String>,
    pub total_bytes: Option<u64>,
    pub free_bytes: Option<u64>,
    pub removable: bool,
    pub device_id: Option<String>,
    pub matched_by: Option<VolumeMatch>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VolumeMatch {
    HwSerial,
    Marker,
    VolumeUuid,
    Mapping,
}

pub fn list_volumes(store: &Store) -> Vec<VolumeInfo> {
    let devices = store.list::<Device>().unwrap_or_else(|err| {
        log::warn!("failed to list devices: {err}");
        Vec::new()
    });
    let mappings = store.list::<DeviceMapping>().unwrap_or_else(|err| {
        log::warn!("failed to list device mappings: {err}");
        Vec::new()
    });
    let computer_id = store.computer_id().to_string();

    enumerate_probes()
        .into_iter()
        .map(|probe| {
            let marker = read_marker(Path::new(&probe.mount_path));
            let matched =
                identify_volume(&probe, marker.as_ref(), &devices, &mappings, &computer_id);
            if let Some((device_id, _)) = &matched {
                upsert_mapping(store, device_id, &computer_id, &probe.mount_path);
            }
            VolumeInfo::from_probe(probe, matched)
        })
        .collect()
}

impl VolumeInfo {
    pub(crate) fn from_probe(probe: ProbeInfo, matched: Option<(String, VolumeMatch)>) -> Self {
        let (device_id, matched_by) = matched
            .map(|(id, by)| (Some(id), Some(by)))
            .unwrap_or((None, None));
        Self {
            mount_path: probe.mount_path,
            name: probe.name,
            volume_uuid: probe.volume_uuid,
            hw_serial: probe.hw_serial,
            total_bytes: probe.total_bytes,
            free_bytes: probe.free_bytes,
            removable: probe.removable,
            device_id,
            matched_by,
        }
    }
}

pub(crate) fn upsert_mapping(store: &Store, device_id: &str, computer_id: &str, root_path: &str) {
    let id = format!("{device_id}@{computer_id}");
    let mapping = DeviceMapping {
        id: id.clone(),
        device_id: device_id.into(),
        computer_id: computer_id.into(),
        root_path: root_path.into(),
    };
    match store.get::<DeviceMapping>(&id) {
        Ok(Some(existing)) if existing.root_path == root_path => {}
        Ok(_) => {
            if let Err(err) = store.put(&mapping) {
                log::warn!("failed to save device mapping {id}: {err}");
            }
        }
        Err(err) => log::warn!("failed to read device mapping {id}: {err}"),
    }
}

pub(crate) fn enumerate_probes() -> Vec<ProbeInfo> {
    let disks = Disks::new_with_refreshed_list();
    disks
        .list()
        .iter()
        .filter_map(|disk| {
            let mount_path = disk.mount_point().to_string_lossy().to_string();
            if skip_mount(&mount_path) {
                return None;
            }
            let name = disk.name().to_string_lossy().to_string();
            let mut probe = ProbeInfo {
                mount_path: mount_path.clone(),
                name: if name.is_empty() {
                    mount_path.clone()
                } else {
                    name
                },
                total_bytes: Some(disk.total_space()),
                free_bytes: Some(disk.available_space()),
                removable: disk.is_removable(),
                ..Default::default()
            };
            merge_os_probe(&mut probe);
            Some(probe)
        })
        .collect()
}

#[cfg(target_os = "macos")]
fn merge_os_probe(probe: &mut ProbeInfo) {
    if let Some(os) = super::probe_macos::probe_path(&probe.mount_path) {
        merge_probe(probe, os);
    }
}

#[cfg(target_os = "windows")]
fn merge_os_probe(probe: &mut ProbeInfo) {
    if let Some(os) = super::probe_windows::probe_path(&probe.mount_path) {
        merge_probe(probe, os);
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn merge_os_probe(_probe: &mut ProbeInfo) {}

pub(crate) fn probe_path(path: &str) -> ProbeInfo {
    let mut probe = enumerate_probes()
        .into_iter()
        .filter(|p| path.starts_with(&p.mount_path))
        .max_by_key(|p| p.mount_path.len())
        .unwrap_or_else(|| ProbeInfo {
            mount_path: path.into(),
            name: path.into(),
            ..Default::default()
        });
    #[cfg(target_os = "macos")]
    if let Some(os) = super::probe_macos::probe_path(path) {
        merge_probe(&mut probe, os);
    }
    #[cfg(target_os = "windows")]
    if let Some(os) = super::probe_windows::probe_path(path) {
        merge_probe(&mut probe, os);
    }
    probe
}

fn merge_probe(target: &mut ProbeInfo, os: ProbeInfo) {
    target.name = first_non_empty(os.name, target.name.clone());
    target.volume_uuid = os.volume_uuid.or_else(|| target.volume_uuid.clone());
    target.hw_serial = os.hw_serial.or_else(|| target.hw_serial.clone());
    target.total_bytes = os.total_bytes.or(target.total_bytes);
    target.free_bytes = os.free_bytes.or(target.free_bytes);
    target.removable |= os.removable;
}

fn first_non_empty(first: String, second: String) -> String {
    if first.is_empty() {
        second
    } else {
        first
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn skip_mount(path: &str) -> bool {
    path == "/" || path.starts_with("/System/Volumes/") || path.starts_with("/private/var/vm")
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn skip_mount(_path: &str) -> bool {
    false
}
