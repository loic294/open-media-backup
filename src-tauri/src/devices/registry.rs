use super::{
    marker::write_marker,
    volumes::{probe_path, upsert_mapping},
    DeviceError,
};
use crate::{
    domain::{new_id, Device},
    store::Store,
};
use std::path::{Path, PathBuf};

pub fn register_device(
    store: &Store,
    mount_path: &str,
    mut device: Device,
) -> Result<Device, DeviceError> {
    if device.id.is_empty() {
        device.id = new_id();
    }
    refresh_device_hints(&mut device, mount_path, false);
    if let Err(err) = write_marker(Path::new(mount_path), &device) {
        log::warn!("failed to write device marker at {mount_path}: {err}");
    }
    store.put(&device)?;
    upsert_mapping(store, &device.id, store.computer_id(), mount_path);
    Ok(device)
}

pub fn relink_device(
    store: &Store,
    device_id: &str,
    mount_path: &str,
) -> Result<Device, DeviceError> {
    let mut device = store
        .get::<Device>(device_id)?
        .ok_or_else(|| DeviceError::NotFound(device_id.into()))?;
    refresh_device_hints(&mut device, mount_path, true);
    if let Err(err) = write_marker(Path::new(mount_path), &device) {
        log::warn!("failed to write device marker at {mount_path}: {err}");
    }
    store.put(&device)?;
    upsert_mapping(store, &device.id, store.computer_id(), mount_path);
    Ok(device)
}

pub fn resolve_root(store: &Store, device_id: &str) -> Option<PathBuf> {
    let id = format!("{}@{}", device_id, store.computer_id());
    let mapping = store
        .get::<crate::domain::DeviceMapping>(&id)
        .ok()
        .flatten()?;
    let path = PathBuf::from(mapping.root_path);
    path.exists().then_some(path)
}

fn refresh_device_hints(device: &mut Device, mount_path: &str, overwrite: bool) {
    let probe = probe_path(mount_path);
    if device.name.is_empty() {
        device.name = probe.name;
    }
    if overwrite || device.hw_serial.is_none() {
        device.hw_serial = probe.hw_serial;
    }
    if overwrite || device.volume_uuid.is_none() {
        device.volume_uuid = probe.volume_uuid;
    }
    if overwrite || device.capacity_bytes.is_none() {
        device.capacity_bytes = probe.total_bytes;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;

    #[test]
    fn registers_arbitrary_folder_and_mapping() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open_in_memory().unwrap();
        let device = register_device(
            &store,
            dir.path().to_str().unwrap(),
            Device {
                name: "NAS".into(),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(!device.id.is_empty());
        assert_eq!(resolve_root(&store, &device.id).unwrap(), dir.path());
    }

    #[test]
    fn relink_missing_device_errors() {
        let store = Store::open_in_memory().unwrap();
        assert!(matches!(
            relink_device(&store, "missing", "."),
            Err(DeviceError::NotFound(_))
        ));
    }
}
