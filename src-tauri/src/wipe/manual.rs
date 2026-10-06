use crate::domain::{Device, DeviceRole, FileCopy, Source};
use crate::store::Store;

/// Acknowledges an external format without touching the filesystem or claiming new backups.
pub fn mark_manually_wiped(store: &Store, source_id: &str) -> Result<(), String> {
    let source: Source = store
        .get(source_id)
        .map_err(|error| error.to_string())?
        .ok_or("source not found")?;
    let device: Device = store
        .get(&source.device_id)
        .map_err(|error| error.to_string())?
        .ok_or("Select a device in source settings before marking it manually wiped")?;
    if device.role == DeviceRole::Final {
        return Err("Final devices are never wiped".into());
    }
    let copies = store
        .list_by::<FileCopy>("device_id", &device.id)
        .map_err(|error| error.to_string())?;
    let removed = retired_copies(copies, chrono::Utc::now().timestamp_millis());
    store.put_all(&removed).map_err(|error| error.to_string())
}

fn retired_copies(copies: Vec<FileCopy>, now: i64) -> Vec<FileCopy> {
    copies
        .into_iter()
        .filter(|copy| !copy.removed)
        .map(|copy| FileCopy {
            removed: true,
            verified_at: now,
            ..copy
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retiring_copies_preserves_identity_and_skips_already_removed_claims() {
        let live = FileCopy {
            id: "hash@card:DCIM/A.JPG".into(),
            file_id: "hash".into(),
            device_id: "card".into(),
            path: "DCIM/A.JPG".into(),
            verified_at: 1,
            removed: false,
        };
        let removed = FileCopy {
            removed: true,
            ..live.clone()
        };
        assert_eq!(
            retired_copies(vec![live.clone(), removed], 42),
            vec![FileCopy {
                removed: true,
                verified_at: 42,
                ..live
            }]
        );
    }
}
