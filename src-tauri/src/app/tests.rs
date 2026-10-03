use super::*;
use crate::domain::{Flow, Source};
use crate::plan::Category;
use crate::testing::{Fixture, MapResolver};
use serde_json::json;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn core(fx: &Fixture) -> (AppCore, tempfile::TempDir) {
    let roots = fx.resolver.0.lock().clone();
    let thumbs = tempfile::tempdir().unwrap();
    let core = AppCore::new(
        fx.store.clone(),
        Arc::new(MapResolver(parking_lot::Mutex::new(roots))),
        thumbs.path().into(),
        |_| {},
    );
    (core, thumbs)
}

fn wait_idle(core: &AppCore) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while core.transfers.is_busy() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn snapshot_and_settings_round_trip() {
    let fx = Fixture::new();
    let (core, _t) = core(&fx);
    core.register_computer().unwrap();
    let snap = core.snapshot().unwrap();
    assert_eq!(snap.computer.id, fx.store.computer_id());
    assert_eq!(snap.spaces.len(), 1);
    let mut settings = core.settings();
    settings.theme = "dark".into();
    core.save_settings(&settings).unwrap();
    assert_eq!(core.settings().theme, "dark");
}

#[test]
fn delete_cascades_and_guards_devices() {
    let fx = Fixture::new();
    let (core, _t) = core(&fx);
    assert!(core.delete_entity("device", "card").is_err());
    core.delete_entity("source", "src").unwrap();
    assert!(fx.store.list::<Flow>().unwrap().is_empty());
    core.delete_entity("space", "space").unwrap();
    assert!(core.snapshot().unwrap().destinations.is_empty());
    core.delete_entity("device", "card").unwrap();
}

#[test]
fn save_entity_validates() {
    let fx = Fixture::new();
    let (core, _t) = core(&fx);
    core.save_entity(
        "source",
        json!({"id": "s2", "space_id": "space", "device_id": "card"}),
    )
    .unwrap();
    assert!(fx.store.get::<Source>("s2").unwrap().is_some());
    assert!(core.save_entity("file_copy", json!({"id": "x"})).is_err());
    assert!(core
        .save_entity("source", json!({"space_id": "space"}))
        .is_err());
}

#[test]
fn run_all_then_list_files() {
    let fx = Fixture::new();
    fx.write_card_file("DCIM/A.JPG", b"a");
    fx.write_card_file("DCIM/B.MOV", b"bb");
    fx.write_card_file("DCIM/C.JPG", b"c");
    let (core, _t) = core(&fx);
    let req = |category, offset, limit| ListFilesRequest {
        project_id: "project".into(),
        flow_id: "flow".into(),
        category,
        offset,
        limit,
        filter: None,
    };
    let page = core.list_files(&req(Category::ToTransfer, 1, 1)).unwrap();
    assert_eq!((page.total, page.total_bytes, page.items.len()), (3, 4, 1));
    assert_eq!(
        page.items[0].target_path.as_deref(),
        Some("photo/Trip/Camera A Card 1/B.MOV")
    );
    assert_eq!(core.run_all("project").unwrap().len(), 1);
    wait_idle(&core);
    assert_eq!(
        core.list_files(&req(Category::Transferred, 0, 50))
            .unwrap()
            .total,
        3
    );
    assert!(core.run_all("project").unwrap().is_empty());
    let status = core.project_status("project").unwrap();
    assert!(status.sources[0].wipe_eligible);
    core.start_wipe("project", "src", crate::wipe::WipeMethod::DeleteFiles)
        .unwrap();
    wait_idle(&core);
    assert!(!fx.card_dir.path().join("DCIM/A.JPG").exists());
}

#[test]
fn save_project_policy_rejects_overlap_and_none_scope_without_mutation() {
    let fx = Fixture::new();
    let (core, _t) = core(&fx);
    let mut project = fx.project.clone();
    project.start_time = Some(0);
    project.end_time = Some(60_000);
    core.save_entity("project", serde_json::to_value(&project).unwrap())
        .unwrap();
    project.id = "second".into();
    core.save_entity("project", serde_json::to_value(&project).unwrap())
        .unwrap();
    let mut space = fx.space.clone();
    space.allow_project_overlap = false;
    assert!(core
        .save_entity("space", serde_json::to_value(&space).unwrap())
        .unwrap_err()
        .contains("overlap"));
    assert!(
        fx.store
            .get::<crate::domain::Space>("space")
            .unwrap()
            .unwrap()
            .allow_project_overlap
    );

    let mut destination = fx.destination.clone();
    destination.path_template = "{project_name}".into();
    core.save_entity("destination", serde_json::to_value(&destination).unwrap())
        .unwrap();
    let mut source = fx.source.clone();
    source.project_scope = crate::domain::ProjectScope::None;
    assert!(core
        .save_entity("source", serde_json::to_value(&source).unwrap())
        .unwrap_err()
        .contains("cannot use project variables"));
    assert_eq!(
        fx.store
            .get::<Source>("src")
            .unwrap()
            .unwrap()
            .project_scope,
        crate::domain::ProjectScope::All
    );
}
