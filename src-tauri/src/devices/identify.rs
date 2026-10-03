use super::{marker::Marker, volumes::VolumeMatch, ProbeInfo};
use crate::domain::{Device, DeviceMapping};

pub(crate) fn identify_volume(
    probe: &ProbeInfo,
    marker: Option<&Marker>,
    devices: &[Device],
    mappings: &[DeviceMapping],
    computer_id: &str,
) -> Option<(String, VolumeMatch)> {
    if let Some(serial) = probe.hw_serial.as_deref().filter(|s| !s.is_empty()) {
        let mut matches = devices
            .iter()
            .filter(|d| d.hw_serial.as_deref() == Some(serial));
        if let Some(device) = matches.next() {
            if matches.next().is_none() {
                return Some((device.id.clone(), VolumeMatch::HwSerial));
            }
        }
    }

    if let Some(marker) = marker {
        if devices.iter().any(|d| d.id == marker.device_id) {
            return Some((marker.device_id.clone(), VolumeMatch::Marker));
        }
    }

    if let Some(uuid) = probe.volume_uuid.as_deref().filter(|s| !s.is_empty()) {
        let mut matches = devices
            .iter()
            .filter(|d| d.volume_uuid.as_deref() == Some(uuid));
        if let Some(device) = matches.next() {
            if matches.next().is_none() {
                return Some((device.id.clone(), VolumeMatch::VolumeUuid));
            }
        }
    }

    mappings
        .iter()
        .find(|m| m.computer_id == computer_id && same_path(&m.root_path, &probe.mount_path))
        .and_then(|m| {
            devices
                .iter()
                .any(|d| d.id == m.device_id)
                .then(|| (m.device_id.clone(), VolumeMatch::Mapping))
        })
}

pub(crate) fn same_path(a: &str, b: &str) -> bool {
    normalize_path(a) == normalize_path(b)
}

fn normalize_path(path: &str) -> String {
    let trimmed = path.trim_end_matches(['/', '\\']);
    if trimmed.is_empty() {
        path.to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(id: &str, serial: Option<&str>, uuid: Option<&str>) -> Device {
        Device {
            id: id.into(),
            hw_serial: serial.map(str::to_string),
            volume_uuid: uuid.map(str::to_string),
            ..Default::default()
        }
    }

    fn probe(serial: Option<&str>, uuid: Option<&str>) -> ProbeInfo {
        ProbeInfo {
            mount_path: "/Volumes/CARD".into(),
            hw_serial: serial.map(str::to_string),
            volume_uuid: uuid.map(str::to_string),
            ..Default::default()
        }
    }

    #[test]
    fn unique_serial_wins() {
        let got = identify_volume(
            &probe(Some("S1"), Some("V1")),
            None,
            &[device("a", Some("S1"), None)],
            &[],
            "c",
        );
        assert_eq!(got, Some(("a".into(), VolumeMatch::HwSerial)));
    }

    #[test]
    fn ambiguous_serial_falls_back_to_marker() {
        let devices = [device("a", Some("S1"), None), device("b", Some("S1"), None)];
        let marker = Marker {
            device_id: "b".into(),
            name: "B".into(),
            written_at: 1,
        };
        let got = identify_volume(&probe(Some("S1"), None), Some(&marker), &devices, &[], "c");
        assert_eq!(got, Some(("b".into(), VolumeMatch::Marker)));
    }

    #[test]
    fn formatted_card_with_no_marker_or_uuid_is_unknown() {
        let got = identify_volume(
            &probe(None, None),
            None,
            &[device("a", None, Some("old"))],
            &[],
            "c",
        );
        assert_eq!(got, None);
    }

    #[test]
    fn mapping_matches_current_computer_path() {
        let devices = [device("a", None, None)];
        let mappings = [DeviceMapping {
            id: "a@c".into(),
            device_id: "a".into(),
            computer_id: "c".into(),
            root_path: "/Volumes/CARD/".into(),
        }];
        let got = identify_volume(&probe(None, None), None, &devices, &mappings, "c");
        assert_eq!(got, Some(("a".into(), VolumeMatch::Mapping)));
    }
}
