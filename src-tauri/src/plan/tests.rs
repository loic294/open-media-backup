use super::*;
use crate::domain::{FileCopy, FileRecord, FileRule, HashAlgo, RuleAction, RuleSyntax};
use crate::testing::Fixture;

fn record_copy(fx: &Fixture, file: &str, size: u64, device: &str, path: &str) {
    let id = FileRecord::id_for(HashAlgo::Xxh64, file);
    fx.store
        .put(&FileRecord { id: id.clone(), hash: file.into(), size, name: path.into(), ..Default::default() })
        .unwrap();
    fx.store
        .put(&FileCopy { id: FileCopy::id_for(&id, device, path), file_id: id, device_id: device.into(), path: path.into(), ..Default::default() })
        .unwrap();
}

fn status(fx: &Fixture) -> ProjectStatus {
    let catalog = Catalog::load(&fx.store).unwrap();
    project_status(&fx.store, &fx.resolver, &catalog, &fx.project.id, &FailureMap::new()).unwrap()
}

#[test]
fn resolves_paths_with_variables_and_subfolder() {
    let fx = Fixture::new();
    let ctx = resolve_flow(&fx.store, &fx.resolver, "project", "flow").unwrap();
    assert_eq!(ctx.source_folder_rel, "DCIM");
    assert_eq!(ctx.target_rel("100/A.ARW"), "photo/Trip/Camera A Card 1/100/A.ARW");
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
    dst.rules = vec![FileRule { action: RuleAction::Exclude, syntax: RuleSyntax::Glob, pattern: "*.THM".into() }];
    fx.store.put(&dst).unwrap();
    fx.write_card_file("DCIM/100/A.ARW", b"aaaa");
    fx.write_card_file("DCIM/100/B.ARW", b"bbbbbb");
    fx.write_card_file("DCIM/100/A.THM", b"t");
    record_copy(&fx, "hashA", 4, "card", "DCIM/100/A.ARW");
    record_copy(&fx, "hashA", 4, "nas", "photo/Trip/Camera A Card 1/100/A.ARW");

    let s = status(&fx);
    let flow = &s.flows[0];
    assert_eq!((flow.transferred, flow.to_transfer, flow.ignored), (1, 1, 1));
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
