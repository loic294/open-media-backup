use super::*;
use crate::domain::FileCopy;
use crate::plan::FailureMap;
use crate::testing::Fixture;
use crate::transfer::{run_transfer, JobHandle, JobState, TransferJob};
use parking_lot::Mutex;
use std::sync::Arc;

fn handle() -> JobHandle {
    JobHandle::new(
        TransferJob {
            analysis: None,
            id: "w".into(),
            flow_id: "wipe".into(),
            label: String::new(),
            state: JobState::Queued,
            files_done: 0,
            files_total: 0,
            bytes_done: 0,
            bytes_total: 0,
            current_file: None,
            speed_bps: 0,
            bytes_per_sec: None,
            eta_secs: None,
            errors: vec![],
            kind: crate::transfer::JobKind::Transfer,
            pending_conflict: None,
            check_results: None,
        },
        Arc::new(|| {}),
    )
}

fn backed_up() -> Fixture {
    let fx = Fixture::new();
    fx.write_card_file("DCIM/A.JPG", b"a");
    fx.write_card_file("DCIM/B.JPG", b"b");
    run_transfer(
        &fx.store,
        &fx.resolver,
        "project",
        "flow",
        &handle(),
        &Mutex::new(FailureMap::new()),
    )
    .unwrap();
    fx
}

#[test]
fn plan_blocks_until_enough_copies() {
    let fx = Fixture::new();
    fx.write_card_file("DCIM/A.JPG", b"a");
    let plan = plan_wipe(&fx.store, &fx.resolver, "project", "src").unwrap();
    assert!(!plan.eligible);
    assert_eq!(plan.reason.as_deref(), Some("Needs Home NAS"));
    assert!(wipe(
        &fx.store,
        &fx.resolver,
        "project",
        "src",
        WipeMethod::DeleteFiles,
        &handle()
    )
    .is_err());
    assert!(fx.card_dir.path().join("DCIM/A.JPG").exists());
}

#[test]
fn delete_files_after_verification() {
    let fx = backed_up();
    let plan = plan_wipe(&fx.store, &fx.resolver, "project", "src").unwrap();
    assert!(plan.eligible, "{:?}", plan.reason);
    wipe(
        &fx.store,
        &fx.resolver,
        "project",
        "src",
        WipeMethod::DeleteFiles,
        &handle(),
    )
    .unwrap();
    assert!(!fx.card_dir.path().join("DCIM/A.JPG").exists());
    assert!(fx.card_dir.path().join("DCIM").is_dir());
    let copies = fx.store.list::<FileCopy>().unwrap();
    assert!(copies
        .iter()
        .filter(|c| c.device_id == "card")
        .all(|c| c.removed));
    assert!(copies
        .iter()
        .filter(|c| c.device_id == "nas")
        .all(|c| !c.removed));
}

#[test]
fn modified_file_aborts_everything() {
    let fx = backed_up();
    std::fs::write(fx.card_dir.path().join("DCIM/B.JPG"), b"x").unwrap();
    let err = wipe(
        &fx.store,
        &fx.resolver,
        "project",
        "src",
        WipeMethod::DeleteFiles,
        &handle(),
    )
    .unwrap_err();
    assert!(err.contains("B.JPG"), "{err}");
    assert!(fx.card_dir.path().join("DCIM/A.JPG").exists());
}

#[test]
fn sibling_safety_blocks_preview_and_execution_without_expanding_deletion_scope() {
    let fx = backed_up();
    let sibling = crate::domain::Source {
        id: "sibling".into(),
        path_template: "VIDEO".into(),
        ..fx.source.clone()
    };
    fx.store.put(&sibling).unwrap();
    fx.write_card_file("VIDEO/B.MP4", b"video");
    assert!(
        !plan_wipe(&fx.store, &fx.resolver, "project", "src")
            .unwrap()
            .eligible
    );
    assert!(wipe(
        &fx.store,
        &fx.resolver,
        "project",
        "src",
        WipeMethod::DeleteFiles,
        &handle()
    )
    .is_err());
    assert!(fx.card_dir.path().join("DCIM/A.JPG").exists());
    let flow = crate::domain::Flow {
        id: "sibling-flow".into(),
        source_id: sibling.id,
        ..fx.flow.clone()
    };
    fx.store.put(&flow).unwrap();
    run_transfer(
        &fx.store,
        &fx.resolver,
        "project",
        &flow.id,
        &handle(),
        &Mutex::new(FailureMap::new()),
    )
    .unwrap();
    let plan = plan_wipe(&fx.store, &fx.resolver, "project", "src").unwrap();
    assert!(plan.eligible);
    assert_eq!(plan.files_total, 2);
    assert_eq!(plan.copies[0].total, 2);
    wipe(
        &fx.store,
        &fx.resolver,
        "project",
        "src",
        WipeMethod::DeleteFiles,
        &handle(),
    )
    .unwrap();
    assert!(!fx.card_dir.path().join("DCIM/A.JPG").exists());
    assert!(fx.card_dir.path().join("VIDEO/B.MP4").exists());
}

#[test]
fn unresolved_sibling_path_blocks_wipe_preview_and_execution() {
    let fx = backed_up();
    let sibling = crate::domain::Source {
        id: "sibling".into(),
        path_template: "{unknown}".into(),
        ..fx.source.clone()
    };
    fx.store.put(&sibling).unwrap();
    let plan = plan_wipe(&fx.store, &fx.resolver, "project", "src").unwrap();
    assert!(!plan.eligible);
    assert!(plan.reason.unwrap().contains("sibling"));
    assert!(wipe(
        &fx.store,
        &fx.resolver,
        "project",
        "src",
        WipeMethod::QuickFormat,
        &handle()
    )
    .is_err());
    assert!(fx.card_dir.path().join("DCIM/A.JPG").exists());
}
