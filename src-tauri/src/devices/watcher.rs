use super::{list_volumes, VolumeInfo};
use crate::store::Store;
use std::{sync::Arc, thread, time::Duration};

pub fn spawn_watcher(store: Arc<Store>, on_change: impl Fn(Vec<VolumeInfo>) + Send + 'static) {
    thread::spawn(move || {
        let mut previous: Option<Vec<(String, Option<String>)>> = None;
        loop {
            let volumes = list_volumes(&store);
            let current: Vec<_> = volumes
                .iter()
                .map(|v| (v.mount_path.clone(), v.device_id.clone()))
                .collect();
            if previous.as_ref() != Some(&current) {
                previous = Some(current);
                on_change(volumes);
            }
            thread::sleep(Duration::from_secs(3));
        }
    });
}
