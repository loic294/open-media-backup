use super::{volumes::*, ProbeInfo};

#[test]
fn volume_info_carries_match() {
    let probe = ProbeInfo {
        mount_path: "/m".into(),
        name: "m".into(),
        ..Default::default()
    };
    let info = VolumeInfo::from_probe(probe, Some(("dev".into(), VolumeMatch::Marker)));
    assert_eq!(info.device_id.as_deref(), Some("dev"));
    assert_eq!(info.matched_by, Some(VolumeMatch::Marker));
}

#[cfg(target_os = "macos")]
#[test]
fn skips_macos_system_volumes() {
    assert!(skip_mount("/"));
    assert!(skip_mount("/System/Volumes/Data"));
    assert!(!skip_mount("/Volumes/CARD"));
}
