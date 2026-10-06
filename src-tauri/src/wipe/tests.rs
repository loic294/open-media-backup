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
            warnings: vec![],
            remote_hash_active: false,
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
fn delete_wipe_preserves_excluded_unknown_and_changed_files_without_affecting_transfers() {
    use crate::domain::{FileRule, RuleAction, RuleSyntax};
    let fx = backed_up();
    fx.write_card_file("DCIM/PRIVATE/UNKNOWN.JPG", b"never backed up");
    fx.write_card_file("DCIM/B.JPG", b"changed since backup");
    let mut source = fx.source.clone();
    source.safe_copy_rules = vec![
        FileRule::path(RuleAction::Exclude, RuleSyntax::Glob, "B.JPG"),
        FileRule::path(RuleAction::Exclude, RuleSyntax::Glob, "PRIVATE/"),
    ];
    fx.store.put(&source).unwrap();
    let plan = plan_wipe(&fx.store, &fx.resolver, "project", "src").unwrap();
    assert!(plan.eligible);
    assert_eq!((plan.files_total, plan.ignored), (3, 2));
    let ctx = crate::plan::resolve_flow(&fx.store, &fx.resolver, "project", "flow").unwrap();
    let files =
        crate::plan::classify_flow(&ctx, &crate::plan::Catalog::load(&fx.store).unwrap(), None);
    assert!(files
        .iter()
        .any(|file| file.rel_path == "PRIVATE/UNKNOWN.JPG"
            && file.category == crate::plan::Category::ToTransfer));
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
    assert!(fx.card_dir.path().join("DCIM/B.JPG").exists());
    assert!(fx.card_dir.path().join("DCIM/PRIVATE/UNKNOWN.JPG").exists());
    assert!(crate::plan::Catalog::load(&fx.store)
        .unwrap()
        .file_at("card", "DCIM/B.JPG", None)
        .is_some());
}

#[test]
fn exclusions_do_not_weaken_hash_guard_for_still_required_files() {
    use crate::domain::{FileRule, RuleAction, RuleSyntax};
    let fx = backed_up();
    let mut source = fx.source.clone();
    source.safe_copy_rules = vec![FileRule::path(
        RuleAction::Exclude,
        RuleSyntax::Glob,
        "B.JPG",
    )];
    fx.store.put(&source).unwrap();
    fx.write_card_file("DCIM/A.JPG", b"x");
    assert!(wipe(
        &fx.store,
        &fx.resolver,
        "project",
        "src",
        WipeMethod::DeleteFiles,
        &handle()
    )
    .unwrap_err()
    .contains("changed"));
    assert!(fx.card_dir.path().join("DCIM/A.JPG").exists());
    assert!(fx.card_dir.path().join("DCIM/B.JPG").exists());
}

#[test]
fn overlapping_required_view_prevents_exemption_in_wipe_report_and_runtime() {
    use crate::domain::{FileRule, RuleAction, RuleSyntax, Source};
    let fx = backed_up();
    let mut source = fx.source.clone();
    source.safe_copy_rules = vec![FileRule::path(
        RuleAction::Exclude,
        RuleSyntax::Glob,
        "*.JPG",
    )];
    fx.store.put(&source).unwrap();
    fx.store
        .put(&Source {
            id: "parent".into(),
            path_template: "".into(),
            ..fx.source.clone()
        })
        .unwrap();
    let plan = plan_wipe(&fx.store, &fx.resolver, "project", "src").unwrap();
    assert_eq!(plan.ignored, 0);
    assert!(plan.eligible);
    fx.write_card_file("DCIM/A.JPG", b"x");
    assert!(wipe(
        &fx.store,
        &fx.resolver,
        "project",
        "src",
        WipeMethod::DeleteFiles,
        &handle()
    )
    .unwrap_err()
    .contains("changed"));
    assert!(fx.card_dir.path().join("DCIM/B.JPG").exists());
}

#[test]
fn workspace_delete_preserves_files_excluded_in_any_active_project() {
    use crate::domain::{FileRule, Project, RuleExpr};
    let fx = backed_up();
    let mut project = fx.project.clone();
    project.values.insert("keep".into(), "yes".into());
    fx.store.put(&project).unwrap();
    fx.store
        .put(&Project {
            id: "other-project".into(),
            values: std::collections::BTreeMap::from([("keep".into(), "no".into())]),
            ..project.clone()
        })
        .unwrap();
    let mut source = fx.source.clone();
    source.safe_copy_rules = vec![FileRule::condition(RuleExpr::Eq {
        var: "keep".into(),
        value: "yes".into(),
    })];
    fx.store.put(&source).unwrap();
    assert!(
        plan_workspace_wipe(&fx.store, &fx.resolver, "src")
            .unwrap()
            .eligible
    );
    wipe_workspace(
        &fx.store,
        &fx.resolver,
        "src",
        WipeMethod::DeleteFiles,
        &handle(),
    )
    .unwrap();
    assert!(fx.card_dir.path().join("DCIM/A.JPG").exists());
    assert!(fx.card_dir.path().join("DCIM/B.JPG").exists());
}

#[test]
fn manual_wipe_retires_whole_device_and_resets_offline_and_reused_filename_status() {
    use crate::domain::{Flow, Source, Space};
    use crate::plan::{classify_flow, project_status, resolve_flow, Catalog, Category};
    let fx = backed_up();
    let sibling = Source {
        id: "sibling".into(),
        space_id: "other-space".into(),
        path_template: "PRIVATE".into(),
        ..fx.source.clone()
    };
    fx.store
        .put(&Space {
            id: "other-space".into(),
            ..fx.space.clone()
        })
        .unwrap();
    fx.store.put(&sibling).unwrap();
    fx.store
        .put(&Flow {
            id: "sibling-flow".into(),
            source_id: sibling.id.clone(),
            space_id: sibling.space_id.clone(),
            ..fx.flow.clone()
        })
        .unwrap();
    let copy = fx
        .store
        .list::<FileCopy>()
        .unwrap()
        .into_iter()
        .find(|copy| copy.device_id == "card")
        .unwrap();
    fx.store
        .put(&FileCopy {
            id: FileCopy::id_for(&copy.file_id, "card", "PRIVATE/OLD.JPG"),
            path: "PRIVATE/OLD.JPG".into(),
            ..copy
        })
        .unwrap();
    let before = Catalog::load(&fx.store).unwrap();
    assert_eq!(
        project_status(
            &fx.store,
            &fx.resolver,
            &before,
            "project",
            &FailureMap::new()
        )
        .unwrap()
        .sources[0]
            .safe_copies,
        1
    );
    mark_manually_wiped(&fx.store, "src").unwrap();
    let after = Catalog::load(&fx.store).unwrap();
    assert_eq!(after.copies_under("card", "").count(), 0);
    assert_eq!(after.copies_under("nas", "").count(), 2);
    assert!(
        fx.card_dir.path().join("DCIM/A.JPG").exists(),
        "Acknowledgment must not delete files"
    );
    let status = project_status(
        &fx.store,
        &fx.resolver,
        &after,
        "project",
        &FailureMap::new(),
    )
    .unwrap();
    assert_eq!(status.sources[0].safe_copies, 0);
    assert!(!status.sources[0].wipe_eligible);
    let ctx = resolve_flow(&fx.store, &fx.resolver, "project", "flow").unwrap();
    let files = classify_flow(&ctx, &after, None);
    assert!(files
        .iter()
        .all(|file| file.category == Category::ToTransfer && file.file_id.is_none()));
    // A same-size filename reused by the camera must not inherit old verification.
    fx.write_card_file("DCIM/A.JPG", b"x");
    assert!(classify_flow(&ctx, &after, None)
        .iter()
        .all(|file| file.category == Category::ToTransfer));
    fx.unmount("card");
    let offline = resolve_flow(&fx.store, &fx.resolver, "project", "flow").unwrap();
    assert!(classify_flow(&offline, &after, None).is_empty());
    let status = project_status(
        &fx.store,
        &fx.resolver,
        &after,
        "project",
        &FailureMap::new(),
    )
    .unwrap();
    assert_eq!(status.sources[0].file_count, 0);
    assert_eq!(status.sources[0].safe_copies, 0);
}

#[test]
fn manual_wipe_persists_and_syncs_without_project_or_mounted_device() {
    use crate::store::{Store, VersionVector};
    let fx = backed_up();
    fx.unmount("card");
    fx.store
        .delete(crate::domain::EntityKind::Project, "project")
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("catalog.sqlite");
    let persistent = Store::open(&path).unwrap();
    persistent
        .apply_remote(&fx.store.ops_since(&VersionVector::new(), 10000).unwrap())
        .unwrap();
    let peer = Store::open_in_memory().unwrap();
    peer.apply_remote(&persistent.ops_since(&VersionVector::new(), 10000).unwrap())
        .unwrap();
    mark_manually_wiped(&persistent, "src").unwrap();
    peer.apply_remote(
        &persistent
            .ops_since(&peer.version_vector().unwrap(), 10000)
            .unwrap(),
    )
    .unwrap();
    let vv = persistent.version_vector().unwrap();
    mark_manually_wiped(&persistent, "src").unwrap();
    assert_eq!(
        persistent.version_vector().unwrap(),
        vv,
        "Repeated acknowledgment should be idempotent"
    );
    drop(persistent);
    let reopened = Store::open(&path).unwrap();
    assert_eq!(
        reopened.list::<FileCopy>().unwrap(),
        peer.list::<FileCopy>().unwrap()
    );
    assert_eq!(
        crate::plan::Catalog::load(&reopened)
            .unwrap()
            .copies_under("card", "")
            .count(),
        0
    );
    assert_eq!(
        crate::plan::Catalog::load(&peer)
            .unwrap()
            .copies_under("nas", "")
            .count(),
        2
    );
}

#[test]
fn manual_wipe_rejects_missing_and_final_devices_without_mutation() {
    let fx = backed_up();
    let before = fx.store.list::<FileCopy>().unwrap();
    assert!(mark_manually_wiped(&fx.store, "unknown").is_err());
    let detached = crate::domain::Source {
        device_id: "".into(),
        ..fx.source.clone()
    };
    fx.store.put(&detached).unwrap();
    assert!(mark_manually_wiped(&fx.store, "src").is_err());
    let final_source = crate::domain::Source {
        device_id: "nas".into(),
        ..fx.source.clone()
    };
    fx.store.put(&final_source).unwrap();
    assert!(mark_manually_wiped(&fx.store, "src")
        .unwrap_err()
        .contains("Final"));
    assert_eq!(fx.store.list::<FileCopy>().unwrap(), before);
}

#[test]
fn manual_wipe_rolls_back_all_copies_when_persistence_fails() {
    use crate::store::{Store, VersionVector};
    let fx = backed_up();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("catalog.sqlite");
    let store = Store::open(&path).unwrap();
    store
        .apply_remote(&fx.store.ops_since(&VersionVector::new(), 10000).unwrap())
        .unwrap();
    let before = store.list::<FileCopy>().unwrap();
    let vv = store.version_vector().unwrap();
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch("CREATE TRIGGER reject_manual_wipe BEFORE INSERT ON entities WHEN NEW.kind = 'file_copy' BEGIN SELECT RAISE(ABORT, 'catalog write failed'); END;").unwrap();
    assert!(mark_manually_wiped(&store, "src")
        .unwrap_err()
        .contains("catalog write failed"));
    assert_eq!(store.list::<FileCopy>().unwrap(), before);
    assert_eq!(store.version_vector().unwrap(), vv);
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
fn workspace_wipe_checks_each_active_project_and_ignores_archived_projects() {
    let fx = backed_up();
    let mut archived = fx.project.clone();
    archived.id = "archived".into();
    archived.name = "Archived".into();
    archived.archived = true;
    archived.final_copies_required = 5;
    fx.store.put(&archived).unwrap();
    let plan = plan_workspace_wipe(&fx.store, &fx.resolver, "src").unwrap();
    assert!(plan.eligible, "{:?}", plan.reason);

    let mut active = archived;
    active.id = "active".into();
    active.name = "Active".into();
    active.archived = false;
    active.final_copies_required = 2;
    fx.store.put(&active).unwrap();
    let plan = plan_workspace_wipe(&fx.store, &fx.resolver, "src").unwrap();
    assert!(!plan.eligible);
    assert!(plan.reason.unwrap().contains("Active"));

    let mut source = fx.source.clone();
    source.project_scope = crate::domain::ProjectScope::Selected {
        project_ids: vec![fx.project.id.clone()],
    };
    fx.store.put(&source).unwrap();
    let plan = plan_workspace_wipe(&fx.store, &fx.resolver, "src").unwrap();
    assert!(!plan.eligible);
    assert!(plan.reason.unwrap().contains("Active"));
}

#[test]
fn workspace_wipe_blocks_when_active_projects_resolve_different_source_folders() {
    let fx = backed_up();
    let second = crate::domain::Project {
        id: "second".into(),
        name: "Second".into(),
        ..fx.project.clone()
    };
    fx.store.put(&second).unwrap();
    let source = crate::domain::Source {
        path_template: "DCIM/{project}".into(),
        ..fx.source.clone()
    };
    fx.store.put(&source).unwrap();

    let plan = plan_workspace_wipe(&fx.store, &fx.resolver, "src").unwrap();
    assert!(!plan.eligible);
    assert_eq!(
        plan.reason.as_deref(),
        Some("Source path resolves to different folders across active projects")
    );
}

#[test]
fn workspace_wipe_deletes_shared_source_files_after_all_active_projects_pass() {
    let fx = backed_up();
    let second = crate::domain::Project {
        id: "second".into(),
        name: "Second".into(),
        ..fx.project.clone()
    };
    fx.store.put(&second).unwrap();

    wipe_workspace(
        &fx.store,
        &fx.resolver,
        "src",
        WipeMethod::DeleteFiles,
        &handle(),
    )
    .unwrap();

    assert!(!fx.card_dir.path().join("DCIM/A.JPG").exists());
    assert!(!fx.card_dir.path().join("DCIM/B.JPG").exists());
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
