use super::*;
use crate::domain::{
    Destination, DestinationKind, Device, DeviceRole, FileCopy, FileRecord, FileRule, Flow,
    HashAlgo, RuleAction, RuleSyntax, Source,
};
use crate::testing::Fixture;

fn record_copy(fx: &Fixture, file: &str, size: u64, device: &str, path: &str) {
    let id = FileRecord::id_for(HashAlgo::Xxh64, file);
    fx.store
        .put(&FileRecord {
            id: id.clone(),
            hash: file.into(),
            size,
            name: path.into(),
            ..Default::default()
        })
        .unwrap();
    fx.store
        .put(&FileCopy {
            id: FileCopy::id_for(&id, device, path),
            file_id: id,
            device_id: device.into(),
            path: path.into(),
            ..Default::default()
        })
        .unwrap();
}

fn status(fx: &Fixture) -> ProjectStatus {
    let catalog = Catalog::load(&fx.store).unwrap();
    project_status(
        &fx.store,
        &fx.resolver,
        &catalog,
        &fx.project.id,
        &FailureMap::new(),
    )
    .unwrap()
}

#[test]
fn resolves_paths_with_variables_and_subfolder() {
    let fx = Fixture::new();
    let ctx = resolve_flow(&fx.store, &fx.resolver, "project", "flow").unwrap();
    assert_eq!(ctx.source_folder_rel, "DCIM");
    assert_eq!(
        ctx.target_rel("100/A.ARW"),
        "photo/Trip/Camera A Card 1/100/A.ARW"
    );
    assert!(ctx.config_error.is_none());
}

#[test]
fn missing_variable_is_a_config_error() {
    let fx = Fixture::new();
    let mut dst = fx.destination.clone();
    dst.path_template = "{client}/x".into();
    fx.store.put(&dst).unwrap();
    let s = status(&fx);
    assert_eq!(s.flows[0].state, FlowState::Error);
    assert!(s.flows[0].error.as_deref().unwrap().contains("{client}"));
}

#[test]
fn classifies_pending_transferred_and_ignored() {
    let fx = Fixture::new();
    let mut dst = fx.destination.clone();
    dst.rules = vec![FileRule::path(
        RuleAction::Exclude,
        RuleSyntax::Glob,
        "*.THM",
    )];
    fx.store.put(&dst).unwrap();
    fx.write_card_file("DCIM/100/A.ARW", b"aaaa");
    fx.write_card_file("DCIM/100/B.ARW", b"bbbbbb");
    fx.write_card_file("DCIM/100/A.THM", b"t");
    record_copy(&fx, "hashA", 4, "card", "DCIM/100/A.ARW");
    record_copy(
        &fx,
        "hashA",
        4,
        "nas",
        "photo/Trip/Camera A Card 1/100/A.ARW",
    );

    let s = status(&fx);
    let flow = &s.flows[0];
    assert_eq!(
        (flow.transferred, flow.to_transfer, flow.ignored),
        (1, 1, 1)
    );
    assert_eq!(flow.bytes_to_transfer, 6);
    assert_eq!(flow.state, FlowState::Pending);
    let src = &s.sources[0];
    assert_eq!((src.file_count, src.safe_copies), (3, 0));
    assert_eq!(src.blocking_reason.as_deref(), Some("Needs Home NAS"));
}

#[test]
fn fully_copied_source_is_wipe_eligible() {
    let fx = Fixture::new();
    fx.write_card_file("DCIM/A.ARW", b"aaaa");
    record_copy(&fx, "hashA", 4, "card", "DCIM/A.ARW");
    record_copy(&fx, "hashA", 4, "nas", "anywhere/A.ARW");
    let s = status(&fx);
    assert_eq!(s.flows[0].state, FlowState::Done);
    assert_eq!(s.sources[0].safe_copies, 1);
    assert!(s.sources[0].wipe_eligible);
}

#[test]
fn confirmed_app_import_counts_as_a_final_safe_copy_only_when_enabled() {
    let fx = Fixture::new();
    let mut destination = fx.destination.clone();
    destination.kind = DestinationKind::App;
    destination.device_id.clear();
    destination.path_template.clear();
    destination.app_name = Some("Photo app".into());
    destination.counts_as_safe_copy = false;
    fx.store.put(&destination).unwrap();
    fx.write_card_file("DCIM/A.ARW", b"aaaa");
    record_copy(&fx, "hashA", 4, "card", "DCIM/A.ARW");
    record_copy(&fx, "hashA", 4, &destination.id, "A.ARW");

    assert_eq!(status(&fx).sources[0].safe_copies, 0);

    destination.counts_as_safe_copy = true;
    fx.store.put(&destination).unwrap();
    let source = &status(&fx).sources[0];
    assert_eq!(source.safe_copies, 1);
    assert!(source.wipe_eligible);
}

#[test]
fn temporary_copies_count_in_space_configured_groups() {
    let fx = Fixture::new();
    let mut project = fx.project.clone();
    project.final_copies_required = 2;
    fx.store.put(&project).unwrap();
    let mut space = fx.space.clone();
    space.temporary_copies_per_final = 2;
    fx.store.put(&space).unwrap();
    fx.write_card_file("DCIM/A.ARW", b"aaaa");
    record_copy(&fx, "hashA", 4, "card", "DCIM/A.ARW");
    record_copy(&fx, "hashA", 4, "nas", "anywhere/A.ARW");

    for i in 1..=3 {
        let device = Device {
            id: format!("ssd{i}"),
            name: format!("Travel SSD {i}"),
            role: DeviceRole::Temporary,
            ..Default::default()
        };
        let destination = Destination {
            id: format!("tmp{i}"),
            space_id: fx.space.id.clone(),
            device_id: device.id.clone(),
            path_template: format!("tmp{i}"),
            ..Default::default()
        };
        let flow = Flow {
            id: format!("tmp-flow{i}"),
            space_id: fx.space.id.clone(),
            source_id: fx.source.id.clone(),
            destination_id: destination.id.clone(),
        };
        fx.store.put(&device).unwrap();
        fx.store.put(&destination).unwrap();
        fx.store.put(&flow).unwrap();
        record_copy(&fx, "hashA", 4, &device.id, "anywhere/A.ARW");
    }

    let s = status(&fx);
    assert_eq!(s.sources[0].safe_copies, 2);
    assert!(s.sources[0].wipe_eligible);

    space.temporary_copies_per_final = 0;
    fx.store.put(&space).unwrap();
    let s = status(&fx);
    assert_eq!(s.sources[0].safe_copies, 1);
    assert!(!s.sources[0].wipe_eligible);
}

#[test]
fn temporary_source_requires_a_final_copy_before_wiping() {
    let fx = Fixture::new();
    let mut space = fx.space.clone();
    space.temporary_copies_per_final = 2;
    fx.store.put(&space).unwrap();
    let mut ssd_source_device = Device {
        id: "source-ssd".into(),
        name: "Travel SSD".into(),
        role: DeviceRole::Temporary,
        ..Default::default()
    };
    let temp_dest_device = Device {
        id: "other-ssd".into(),
        name: "Other SSD".into(),
        role: DeviceRole::Temporary,
        ..Default::default()
    };
    ssd_source_device.kind = crate::domain::DeviceKind::Ssd;
    fx.store
        .put_all(&[ssd_source_device.clone(), temp_dest_device.clone()])
        .unwrap();
    fx.resolver.0.lock().insert(
        ssd_source_device.id.clone(),
        fx.card_dir.path().to_path_buf(),
    );
    let source = Source {
        id: "ssd-source".into(),
        space_id: fx.space.id.clone(),
        device_id: ssd_source_device.id.clone(),
        path_template: "DCIM".into(),
        offer_wipe: true,
        position: 1,
        ..Default::default()
    };
    let destination = Destination {
        id: "other-ssd-dst".into(),
        space_id: fx.space.id.clone(),
        device_id: temp_dest_device.id.clone(),
        path_template: "copy".into(),
        ..Default::default()
    };
    let flow = Flow {
        id: "other-ssd-flow".into(),
        space_id: fx.space.id.clone(),
        source_id: source.id.clone(),
        destination_id: destination.id.clone(),
    };
    fx.store.put(&source).unwrap();
    fx.store.put(&destination).unwrap();
    fx.store.put(&flow).unwrap();
    fx.write_card_file("DCIM/A.ARW", b"aaaa");
    record_copy(&fx, "hashA", 4, &ssd_source_device.id, "DCIM/A.ARW");
    record_copy(&fx, "hashA", 4, &temp_dest_device.id, "copy/A.ARW");

    let s = status(&fx);
    let source_status = s
        .sources
        .iter()
        .find(|status| status.source_id == source.id)
        .unwrap();
    assert!(!source_status.wipe_eligible);
    assert_eq!(
        source_status.blocking_reason.as_deref(),
        Some("Needs a final destination")
    );

    record_copy(&fx, "hashA", 4, "nas", "copy/A.ARW");
    let s = status(&fx);
    assert!(
        s.sources
            .iter()
            .find(|status| status.source_id == source.id)
            .unwrap()
            .wipe_eligible
    );
}

#[test]
fn offline_source_uses_catalog_and_changed_size_is_unknown() {
    let fx = Fixture::new();
    record_copy(&fx, "hashA", 4, "card", "DCIM/A.ARW");
    fx.unmount("card");
    let s = status(&fx);
    assert!(!s.sources[0].available);
    assert_eq!(s.sources[0].file_count, 1);
    assert_eq!(s.flows[0].state, FlowState::Unavailable);

    let fx = Fixture::new();
    fx.write_card_file("DCIM/A.ARW", b"changed!");
    record_copy(&fx, "hashA", 4, "card", "DCIM/A.ARW");
    record_copy(&fx, "hashA", 4, "nas", "x/A.ARW");
    assert_eq!(status(&fx).flows[0].to_transfer, 1);
}

#[test]
fn backup_marker_supplies_backup_folder() {
    let fx = Fixture::new();
    let mut dst = fx.destination.clone();
    dst.use_backup_marker = true;
    dst.subfolder_per_source = false;
    dst.path_template = "{backup_folder}".into();
    fx.store.put(&dst).unwrap();
    crate::paths::ensure_backup_folder(fx.card_dir.path(), "2026-01-01_Trip").unwrap();
    let ctx = resolve_flow(&fx.store, &fx.resolver, "project", "flow").unwrap();
    assert_eq!(ctx.target_rel("A.ARW"), "2026-01-01_Trip/A.ARW");
}
