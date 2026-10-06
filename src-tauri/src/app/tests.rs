use super::*;
use crate::domain::{Destination, DestinationKind, Flow, Source};
use crate::plan::Category;
use crate::testing::{Fixture, MapResolver};
use serde_json::json;
use std::fs;
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
fn safe_copy_rules_require_local_mapping_context_and_valid_rules_and_sync_as_source() {
    use crate::domain::{DeviceMapping, FileRule, RuleAction, RuleSyntax};
    use crate::plan::WorkspaceContext;
    use crate::store::{Store, VersionVector};
    let fx = Fixture::new();
    let (core, _t) = core(&fx);
    let context = WorkspaceContext::for_project(&fx.store, "project").unwrap();
    let rules = vec![FileRule::path(
        RuleAction::Exclude,
        RuleSyntax::Glob,
        "*.THM",
    )];
    assert!(
        !core
            .source_safe_copy_details(&context, "src")
            .unwrap()
            .editable
    );
    assert!(core
        .save_source_safe_copy_rules(&context, "src", rules.clone())
        .is_err());
    let mut mapping = DeviceMapping {
        id: format!("card@{}", fx.store.computer_id()),
        device_id: "card".into(),
        computer_id: "peer".into(),
        root_path: fx.card_dir.path().display().to_string(),
    };
    fx.store.put(&mapping).unwrap();
    assert!(core
        .save_source_safe_copy_rules(&context, "src", rules.clone())
        .is_err());
    mapping.computer_id = fx.store.computer_id().into();
    fx.store.put(&mapping).unwrap();
    assert!(
        core.source_safe_copy_details(&context, "src")
            .unwrap()
            .editable
    );
    let wrong = WorkspaceContext {
        space_id: "missing".into(),
        project_id: Some("project".into()),
    };
    assert!(core
        .save_source_safe_copy_rules(&wrong, "src", rules.clone())
        .is_err());
    assert!(core
        .save_source_safe_copy_rules(&context, "/arbitrary/path", rules.clone())
        .is_err());
    assert!(core
        .save_source_safe_copy_rules(
            &context,
            "src",
            vec![FileRule::path(RuleAction::Exclude, RuleSyntax::Regex, "(")]
        )
        .is_err());
    assert!(fx
        .store
        .get::<Source>("src")
        .unwrap()
        .unwrap()
        .safe_copy_rules
        .is_empty());
    core.save_source_safe_copy_rules(&context, "src", rules.clone())
        .unwrap();
    let source: Source = fx.store.get("src").unwrap().unwrap();
    assert_eq!(source.safe_copy_rules, rules);
    let peer = Store::open_in_memory().unwrap();
    peer.apply_remote(&fx.store.ops_since(&VersionVector::new(), 10000).unwrap())
        .unwrap();
    assert_eq!(
        peer.get::<Source>("src").unwrap().unwrap().safe_copy_rules,
        rules
    );
    let mut old_json = serde_json::to_value(&source).unwrap();
    old_json.as_object_mut().unwrap().remove("safe_copy_rules");
    assert!(serde_json::from_value::<Source>(old_json)
        .unwrap()
        .safe_copy_rules
        .is_empty());
    let mut bypass = source.clone();
    bypass.safe_copy_rules.clear();
    assert!(core
        .save_entity("source", serde_json::to_value(bypass).unwrap())
        .unwrap_err()
        .contains("save_source_safe_copy_rules"));
    let mut moved = source.clone();
    moved.path_template = "PRIVATE".into();
    assert!(core
        .save_entity("source", serde_json::to_value(moved).unwrap())
        .is_err());
    core.save_entity("source", serde_json::to_value(source).unwrap())
        .unwrap();
}

#[test]
fn safe_copy_details_share_sorted_device_scope_and_do_not_discredit_offline_verified_copies() {
    use crate::domain::{FileCopy, FileRecord, HashAlgo};
    let fx = Fixture::new();
    fx.write_card_file("DCIM/Z.JPG", b"z");
    fx.write_card_file("VIDEO/A.MP4", b"a");
    fx.store
        .put(&Source {
            id: "sibling".into(),
            path_template: "VIDEO".into(),
            ..fx.source.clone()
        })
        .unwrap();
    for (path, hash) in [("DCIM/Z.JPG", "z"), ("VIDEO/A.MP4", "a")] {
        let id = FileRecord::id_for(HashAlgo::Xxh64, hash);
        fx.store
            .put(&FileRecord {
                id: id.clone(),
                hash: hash.into(),
                size: 1,
                ..Default::default()
            })
            .unwrap();
        for device in ["card", "nas"] {
            fx.store
                .put(&FileCopy {
                    id: FileCopy::id_for(&id, device, path),
                    file_id: id.clone(),
                    device_id: device.into(),
                    path: path.into(),
                    ..Default::default()
                })
                .unwrap();
        }
    }
    fx.unmount("nas");
    let (core, _t) = core(&fx);
    let context = crate::plan::WorkspaceContext::for_project(&fx.store, "project").unwrap();
    let details = core.source_safe_copy_details(&context, "src").unwrap();
    assert_eq!(
        details
            .files
            .iter()
            .map(|file| file.path.as_str())
            .collect::<Vec<_>>(),
        vec!["DCIM/Z.JPG", "VIDEO/A.MP4"]
    );
    assert!(details.wipe_eligible);
    assert_eq!(details.safe_copies, 1);
    assert!(details
        .files
        .iter()
        .all(|file| file.state == crate::plan::SafeCopyState::Safe
            && file.verified_destinations == vec!["Home NAS"]
            && !file.reasons.iter().any(|reason| reason.contains("offline"))));
    let status = core.workspace_status(&context).unwrap();
    assert_eq!(status.sources[0].safe_copies, details.safe_copies);
    assert_eq!(status.sources[0].blocking_reason, details.blocking_reason);
}

#[test]
fn file_safety_is_independent_of_pending_siblings_and_uses_space_policy() {
    use crate::domain::{FileCopy, FileRecord, HashAlgo};
    let fx = Fixture::new();
    fx.write_card_file("DCIM/SAFE.JPG", b"a");
    let id = FileRecord::id_for(HashAlgo::Xxh64, "a");
    fx.store
        .put(&FileRecord {
            id: id.clone(),
            hash: "a".into(),
            size: 1,
            ..Default::default()
        })
        .unwrap();
    for device in ["card", "nas"] {
        fx.store
            .put(&FileCopy {
                id: FileCopy::id_for(&id, device, "DCIM/SAFE.JPG"),
                file_id: id.clone(),
                device_id: device.into(),
                path: "DCIM/SAFE.JPG".into(),
                ..Default::default()
            })
            .unwrap();
    }
    let (core, _t) = core(&fx);
    let context = crate::plan::WorkspaceContext::for_project(&fx.store, "project").unwrap();
    assert!(
        core.source_safe_copy_details(&context, "src")
            .unwrap()
            .wipe_eligible
    );
    let mut strict = fx.project.clone();
    strict.id = "strict".into();
    fx.store.put(&strict).unwrap();
    assert!(
        core.source_safe_copy_details(&context, "src")
            .unwrap()
            .wipe_eligible
    );
    let mut space = fx.space.clone();
    space.final_copies_required = 2;
    fx.store.put(&space).unwrap();
    let details = core.source_safe_copy_details(&context, "src").unwrap();
    assert!(!details.wipe_eligible);
    assert_eq!(details.files[0].state, crate::plan::SafeCopyState::Unsafe);
    assert_eq!(
        details.blocking_reason,
        crate::wipe::plan_workspace_wipe(&fx.store, core.resolver.as_ref(), "src")
            .unwrap()
            .reason
    );
    fx.write_card_file("DCIM/PENDING.JPG", b"pending");
    fx.store.put(&fx.space).unwrap();
    let details = core.source_safe_copy_details(&context, "src").unwrap();
    assert_eq!(
        details
            .files
            .iter()
            .find(|file| file.path.ends_with("SAFE.JPG"))
            .unwrap()
            .state,
        crate::plan::SafeCopyState::Safe
    );
    assert_eq!(
        details
            .files
            .iter()
            .find(|file| file.path.ends_with("PENDING.JPG"))
            .unwrap()
            .state,
        crate::plan::SafeCopyState::Unsafe
    );
}

#[test]
fn manual_wipe_rejects_device_jobs_and_clears_only_related_failures_after_success() {
    use crate::transfer::{JobKind, JobSpec, ResourceClaim};
    use std::collections::HashMap;
    let fx = Fixture::new();
    let (core, _t) = core(&fx);
    core.failures.lock().insert(
        "flow".into(),
        HashMap::from([("OLD.JPG".into(), "old error".into())]),
    );
    core.failures.lock().insert(
        "unrelated".into(),
        HashMap::from([("KEEP.JPG".into(), "keep error".into())]),
    );
    let (started, wait_started) = std::sync::mpsc::channel();
    let (release, wait_release) = std::sync::mpsc::channel();
    core.transfers.enqueue(JobSpec {
        key: "busy".into(),
        label: "busy".into(),
        resources: vec![ResourceClaim::shared("device:card")],
        kind: JobKind::Check,
        queue: None,
        work: Box::new(move |_| {
            started.send(()).unwrap();
            wait_release.recv().unwrap();
            Ok(())
        }),
    });
    wait_started.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(core
        .mark_source_manually_wiped("src")
        .unwrap_err()
        .contains("active jobs"));
    assert!(core.failures.lock().contains_key("flow"));
    release.send(()).unwrap();
    wait_idle(&core);
    core.mark_source_manually_wiped("src").unwrap();
    assert!(!core.failures.lock().contains_key("flow"));
    assert!(core.failures.lock().contains_key("unrelated"));
}

#[test]
fn manual_wipe_blocks_pending_app_imports_and_resets_confirmed_app_coverage() {
    let fx = Fixture::new();
    fx.write_card_file("DCIM/A.JPG", b"photo");
    let mut destination = fx.destination.clone();
    destination.kind = DestinationKind::App;
    destination.device_id.clear();
    destination.path_template.clear();
    destination.app_name = Some("Photo app".into());
    destination.counts_as_safe_copy = true;
    fx.store.put(&destination).unwrap();
    let (core, _t) = core(&fx);
    let mut settings = core.settings();
    settings.app_destinations.insert(
        destination.id.clone(),
        fake_app(fx.card_dir.path()).to_string_lossy().into_owned(),
    );
    core.save_settings(&settings).unwrap();
    let prepared = core.prepare_app_import("project", "flow").unwrap();
    assert!(core
        .mark_source_manually_wiped("src")
        .unwrap_err()
        .contains("pending app imports"));
    assert_eq!(
        core.confirm_app_import("project", "flow", &prepared.token)
            .unwrap(),
        1
    );
    assert_eq!(
        core.project_status("project").unwrap().sources[0].safe_copies,
        1
    );
    core.mark_source_manually_wiped("src").unwrap();
    let status = core.project_status("project").unwrap();
    assert_eq!(status.sources[0].safe_copies, 0);
    assert_eq!(status.flows[0].transferred, 0);
    assert_eq!(status.flows[0].to_transfer, 1);
    assert!(core
        .store
        .list::<crate::domain::FileCopy>()
        .unwrap()
        .iter()
        .any(|copy| copy.device_id == "dst" && !copy.removed));
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
fn status_io_does_not_hold_the_transfer_failures_mutex() {
    use crate::plan::RootResolver;
    use std::sync::mpsc;
    struct BlockingResolver {
        roots: MapResolver,
        entered: mpsc::Sender<()>,
        resume: std::sync::Mutex<mpsc::Receiver<()>>,
        first: std::sync::atomic::AtomicBool,
    }
    impl RootResolver for BlockingResolver {
        fn device_root(&self, id: &str) -> Option<std::path::PathBuf> {
            if self.first.swap(false, std::sync::atomic::Ordering::SeqCst) {
                self.entered.send(()).unwrap();
                self.resume.lock().unwrap().recv().unwrap();
            }
            self.roots.device_root(id)
        }
    }
    let fx = Fixture::new();
    let thumbs = tempfile::tempdir().unwrap();
    let (entered, observe) = mpsc::channel();
    let (resume, wait) = mpsc::channel();
    let core = Arc::new(AppCore::new(
        fx.store.clone(),
        Arc::new(BlockingResolver {
            roots: MapResolver(parking_lot::Mutex::new(fx.resolver.0.lock().clone())),
            entered,
            resume: std::sync::Mutex::new(wait),
            first: std::sync::atomic::AtomicBool::new(true),
        }),
        thumbs.path().into(),
        |_| {},
    ));
    let worker = core.clone();
    let running = std::thread::spawn(move || worker.project_status("project"));
    observe.recv_timeout(Duration::from_secs(5)).unwrap();
    let lock_available = core.failures.try_lock().is_some();
    resume.send(()).unwrap();
    running.join().unwrap().unwrap();
    assert!(
        lock_available,
        "Status disk I/O must not serialize all other status, preview and transfer work"
    );
}

#[test]
fn delete_cascades_and_detaches_devices() {
    let fx = Fixture::new();
    let (core, _t) = core(&fx);
    core.delete_entity("device", "card").unwrap();
    assert!(fx
        .store
        .get::<Source>("src")
        .unwrap()
        .unwrap()
        .device_id
        .is_empty());
    assert_eq!(fx.store.list::<Flow>().unwrap(), vec![fx.flow.clone()]);
    core.delete_entity("source", "src").unwrap();
    assert!(fx.store.list::<Flow>().unwrap().is_empty());
    core.delete_entity("space", "space").unwrap();
    assert!(core.snapshot().unwrap().destinations.is_empty());
    core.delete_entity("device", "card").unwrap();
}

#[test]
fn deleted_device_blocks_its_tasks_without_breaking_other_flows_and_can_be_reassigned() {
    use crate::domain::{Device, DeviceRole};
    use crate::plan::{FlowState, WorkspaceContext};
    let fx = Fixture::new();
    let file = fx.write_card_file("DCIM/A.JPG", b"photo");
    let replacement = Device {
        id: "replacement".into(),
        name: "Replacement card".into(),
        role: DeviceRole::Original,
        ..Default::default()
    };
    fx.store.put(&replacement).unwrap();
    fx.resolver
        .0
        .lock()
        .insert(replacement.id.clone(), fx.card_dir.path().into());
    let mut other_source = fx.source.clone();
    other_source.id = "other-source".into();
    other_source.device_id = replacement.id.clone();
    fx.store.put(&other_source).unwrap();
    let mut other_flow = fx.flow.clone();
    other_flow.id = "other-flow".into();
    other_flow.source_id = other_source.id;
    fx.store.put(&other_flow).unwrap();
    let (core, _t) = core(&fx);
    core.delete_entity("device", "card").unwrap();
    let context = WorkspaceContext {
        space_id: fx.space.id.clone(),
        project_id: None,
    };
    for status in [
        core.workspace_status(&context).unwrap().flows,
        core.project_status("project").unwrap().flows,
    ] {
        let affected = status.iter().find(|f| f.flow_id == "flow").unwrap();
        assert_eq!(affected.state, FlowState::Unavailable);
        assert!(!affected.runnable);
        assert!(affected.error.as_ref().unwrap().contains("source settings"));
        assert!(
            status
                .iter()
                .find(|f| f.flow_id == "other-flow")
                .unwrap()
                .runnable
        );
    }
    let source_status = core
        .project_status("project")
        .unwrap()
        .sources
        .into_iter()
        .find(|s| s.source_id == "src")
        .unwrap();
    assert!(!source_status.available);
    assert!(!source_status.wipe_eligible);
    assert!(core
        .run_flow("project", "flow")
        .unwrap_err()
        .contains("source"));
    assert!(core
        .list_files(&ListFilesRequest {
            project_id: "project".into(),
            flow_id: "flow".into(),
            category: Category::ToTransfer,
            offset: 0,
            limit: 50,
            filter: None,
            directory: None,
        })
        .unwrap_err()
        .contains("source"));
    assert!(core
        .start_wipe("project", "src", crate::wipe::WipeMethod::DeleteFiles)
        .is_err());
    assert!(file.exists());
    assert_eq!(core.run_all("project").unwrap().len(), 1);
    wait_idle(&core);
    let mut source = fx.store.get::<Source>("src").unwrap().unwrap();
    source.device_id = replacement.id;
    core.save_entity("source", serde_json::to_value(&source).unwrap())
        .unwrap();
    assert!(
        core.project_status("project")
            .unwrap()
            .sources
            .iter()
            .find(|s| s.source_id == "src")
            .unwrap()
            .available
    );
}

#[test]
fn deleted_destination_keeps_flows_and_app_destinations_do_not_need_devices() {
    use crate::plan::FlowState;
    let fx = Fixture::new();
    fx.write_card_file("DCIM/A.JPG", b"photo");
    let mut app = fx.destination.clone();
    app.id = "app-dest".into();
    app.kind = DestinationKind::App;
    app.device_id.clear();
    app.app_name = Some("Photo editor".into());
    fx.store.put(&app).unwrap();
    let mut flow = fx.flow.clone();
    flow.id = "app-flow".into();
    flow.destination_id = app.id;
    fx.store.put(&flow).unwrap();
    let (core, _t) = core(&fx);
    core.delete_entity("device", "nas").unwrap();
    let status = core.project_status("project").unwrap();
    let folder = status
        .destinations
        .iter()
        .find(|d| d.destination_id == "dst")
        .unwrap();
    assert!(!folder.available);
    assert!(folder
        .last_error
        .as_ref()
        .unwrap()
        .contains("destination settings"));
    assert_eq!(
        status
            .flows
            .iter()
            .find(|f| f.flow_id == "flow")
            .unwrap()
            .state,
        FlowState::Unavailable
    );
    assert!(
        status
            .flows
            .iter()
            .find(|f| f.flow_id == "app-flow")
            .unwrap()
            .runnable
    );
    assert_eq!(fx.store.list::<Flow>().unwrap().len(), 2);
    assert!(core
        .run_flow("project", "flow")
        .unwrap_err()
        .contains("destination"));
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
        directory: None,
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
fn folder_preview_summarizes_all_routes_before_directory_pagination() {
    let fx = Fixture::new();
    let mut destination = fx.destination.clone();
    destination.path_template = "backup".into();
    destination.subfolder_per_source = false;
    destination.rules = vec![crate::domain::FileRule::path(
        crate::domain::RuleAction::Exclude,
        crate::domain::RuleSyntax::Glob,
        "*.THM",
    )];
    fx.store.put(&destination).unwrap();
    for index in 0..130 {
        fx.write_card_file(&format!("DCIM/100/A{index:03}.JPG"), b"x");
    }
    fx.write_card_file("DCIM/999/nested/B.JPG", b"bb");
    fx.write_card_file("DCIM/excluded/A.THM", b"x");
    let (core, _t) = core(&fx);
    let mut req: ListFilesRequest = serde_json::from_value(json!({
        "projectId": "project", "flowId": "flow", "category": "to_transfer",
        "offset": 0, "limit": 120
    }))
    .unwrap();
    let flat = core.list_files(&req).unwrap();
    assert_eq!(
        (flat.total, flat.items.len(), flat.total_bytes),
        (131, 120, 132)
    );
    assert!(flat
        .directories
        .iter()
        .any(|directory| directory.path == "backup/999/nested" && directory.total == 1));
    assert!(!flat
        .directories
        .iter()
        .any(|directory| directory.path.contains("excluded")));
    req.directory = Some(
        serde_json::from_value(json!({ "kind": "destination", "path": "backup/100" })).unwrap(),
    );
    let first = core.list_files(&req).unwrap();
    assert_eq!((first.total, first.items.len()), (130, 120));
    let root = first
        .directories
        .iter()
        .find(|directory| directory.path.is_empty())
        .unwrap();
    assert_eq!(
        (root.total, root.total_bytes, root.direct_files),
        (131, 132, 0)
    );
    req.offset = 120;
    let last = core.list_files(&req).unwrap();
    assert_eq!(last.items.len(), 10);
    assert!(last.items.iter().all(|file| file
        .target_path
        .as_deref()
        .unwrap()
        .starts_with("backup/100/")));
    req.offset = 0;
    req.directory = None;
    req.filter = Some("backup/999".into());
    let filtered = core.list_files(&req).unwrap();
    assert_eq!(filtered.total, 1);
    assert!(!filtered
        .directories
        .iter()
        .any(|directory| directory.path == "backup/100"));
    req.filter = None;
    req.category = Category::Ignored;
    let ignored = core.list_files(&req).unwrap();
    assert_eq!(ignored.total, 1);
    assert_eq!(
        ignored.items[0].ignore_reason.as_deref(),
        Some("Excluded by destination rule \"*.THM\"")
    );
    assert!(ignored
        .directories
        .iter()
        .all(|directory| matches!(directory.kind, super::files::DirectoryKind::Source)));
}

#[test]
fn folder_preview_retains_destination_hierarchy_from_offline_catalog() {
    let fx = Fixture::new();
    fx.write_card_file("DCIM/100/nested/A.JPG", b"photo");
    let (online, _t) = core(&fx);
    online.run_all("project").unwrap();
    wait_idle(&online);
    fx.unmount("card");
    let (offline, _t) = core(&fx);
    let req: ListFilesRequest = serde_json::from_value(json!({
        "projectId": "project", "flowId": "flow", "category": "transferred",
        "offset": 0, "limit": 120,
        "directory": { "kind": "destination", "path": "photo/Trip/Camera A Card 1/100/nested" }
    }))
    .unwrap();
    let page = offline.list_files(&req).unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.items[0].rel_path, "100/nested/A.JPG");
    assert!(page.items[0].abs_path.is_none());
    assert_eq!(
        page.items[0].target_path.as_deref(),
        Some("photo/Trip/Camera A Card 1/100/nested/A.JPG")
    );
    assert!(page.directories.iter().any(|directory| directory.path
        == "photo/Trip/Camera A Card 1/100/nested"
        && directory.direct_files == 1));
}

#[test]
fn project_free_transfers_preview_verify_and_enable_wipe_under_space_policy() {
    let fx = Fixture::new();
    fx.store
        .delete(crate::domain::EntityKind::Project, "project")
        .unwrap();
    let source = fx.write_card_file("DCIM/A.JPG", b"project-free photo");
    let (core, _t) = core(&fx);
    let context = crate::plan::WorkspaceContext {
        space_id: fx.space.id.clone(),
        project_id: None,
    };
    let req = |category| ListWorkspaceFilesRequest {
        context: context.clone(),
        flow_id: fx.flow.id.clone(),
        category,
        offset: 0,
        limit: 50,
        filter: None,
        directory: None,
    };
    let pending = core
        .list_workspace_files(&req(Category::ToTransfer))
        .unwrap();
    assert_eq!(pending.total, 1);
    assert_eq!(pending.items[0].project_id, None);
    assert_eq!(
        core.authorize_media_open(&source).unwrap(),
        source.canonicalize().unwrap()
    );
    assert_eq!(core.run_workspace_all(&context).unwrap().len(), 1);
    wait_idle(&core);
    assert!(core
        .transfers
        .jobs()
        .iter()
        .all(|j| j.state == crate::transfer::JobState::Done));
    let target = fx.nas_dir.path().join("photo/Trip/Camera A Card 1/A.JPG");
    assert_eq!(fs::read(&target).unwrap(), b"project-free photo");
    assert_eq!(
        crate::hashing::hash_file(&source, fx.space.hash_algo, |_| true).unwrap(),
        crate::hashing::hash_file(&target, fx.space.hash_algo, |_| true).unwrap(),
    );
    let records = fx.store.list::<crate::domain::FileRecord>().unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(fx.store.list::<crate::domain::FileCopy>().unwrap().len(), 2);
    assert_eq!(
        core.list_workspace_files(&req(Category::Transferred))
            .unwrap()
            .total,
        1
    );
    let status = core.workspace_status(&context).unwrap();
    assert_eq!(status.sources[0].safe_copies, 1);
    assert_eq!(status.sources[0].required_copies, Some(1));
    assert!(status.sources[0].wipe_eligible);
    assert!(core
        .start_wipe("project", "src", crate::wipe::WipeMethod::DeleteFiles)
        .is_err());
    assert!(core.run_workspace_all(&context).unwrap().is_empty());
    core.start_workspace_wipe("src", crate::wipe::WipeMethod::DeleteFiles)
        .unwrap();
    wait_idle(&core);
    assert!(!source.exists());
    assert!(fx
        .store
        .list::<crate::domain::Project>()
        .unwrap()
        .is_empty());
}

#[test]
fn project_free_blocked_paths_never_copy_or_create_markers() {
    let fx = Fixture::new();
    fx.store
        .delete(crate::domain::EntityKind::Project, "project")
        .unwrap();
    fx.write_card_file("DCIM/A.JPG", b"photo");
    let mut destination = fx.destination.clone();
    destination.path_template = "{backup_folder}".into();
    destination.use_backup_marker = true;
    fx.store.put(&destination).unwrap();
    let (core, _t) = core(&fx);
    let context = crate::plan::WorkspaceContext {
        space_id: "space".into(),
        project_id: None,
    };
    assert!(core
        .run_workspace_flow(&context, "flow")
        .unwrap_err()
        .contains("backup folder"));
    assert!(core.run_workspace_all(&context).unwrap().is_empty());
    assert!(!fx
        .card_dir
        .path()
        .join(crate::paths::BACKUP_MARKER_FILE)
        .exists());
    assert!(fs::read_dir(fx.nas_dir.path()).unwrap().next().is_none());
    crate::paths::ensure_backup_folder(fx.card_dir.path(), "Existing").unwrap();
    core.run_workspace_flow(&context, "flow").unwrap();
    wait_idle(&core);
    assert_eq!(
        fs::read(fx.nas_dir.path().join("Existing/Camera A Card 1/A.JPG")).unwrap(),
        b"photo"
    );
}

#[test]
fn project_free_app_imports_are_manual_and_confirmations_are_context_bound() {
    let fx = Fixture::new();
    fx.store
        .delete(crate::domain::EntityKind::Project, "project")
        .unwrap();
    fx.write_card_file("DCIM/A.JPG", b"photo");
    let mut destination = fx.destination.clone();
    destination.kind = DestinationKind::App;
    destination.device_id.clear();
    destination.path_template.clear();
    destination.app_name = Some("Photo app".into());
    destination.counts_as_safe_copy = false;
    fx.store.put(&destination).unwrap();
    let (core, _t) = core(&fx);
    let app_path = fake_app(fx.card_dir.path());
    let mut settings = core.settings();
    settings.app_destinations.insert(
        destination.id.clone(),
        app_path.to_string_lossy().into_owned(),
    );
    core.save_settings(&settings).unwrap();
    let context = crate::plan::WorkspaceContext {
        space_id: "space".into(),
        project_id: None,
    };
    assert!(core.run_workspace_all(&context).unwrap().is_empty());
    assert!(core
        .run_workspace_flow(&context, "flow")
        .unwrap_err()
        .contains("manually"));
    let prepared = core.prepare_workspace_app_import(&context, "flow").unwrap();
    assert_eq!(prepared.files[0].project_id, None);
    assert_eq!(
        core.confirm_workspace_app_import(&context, "flow", &prepared.token)
            .unwrap(),
        1
    );
    assert_eq!(
        core.workspace_status(&context).unwrap().destinations[0].transferred,
        1
    );
    assert_eq!(
        core.workspace_status(&context).unwrap().sources[0].safe_copies,
        0
    );
    destination.counts_as_safe_copy = true;
    core.save_entity("destination", serde_json::to_value(&destination).unwrap())
        .unwrap();
    assert_eq!(
        core.workspace_status(&context).unwrap().sources[0].safe_copies,
        1
    );
    assert!(core
        .confirm_workspace_app_import(&context, "flow", &prepared.token)
        .is_err());
    fx.write_card_file("DCIM/B.JPG", b"new photo");
    let prepared = core.prepare_workspace_app_import(&context, "flow").unwrap();
    let other = crate::plan::WorkspaceContext {
        space_id: "other".into(),
        project_id: None,
    };
    assert!(core
        .confirm_workspace_app_import(&other, "flow", &prepared.token)
        .unwrap_err()
        .contains("does not match"));
}

#[test]
fn project_free_transfer_can_generate_a_project_independent_marker() {
    let fx = Fixture::new();
    fx.store
        .delete(crate::domain::EntityKind::Project, "project")
        .unwrap();
    fx.write_card_file("DCIM/A.JPG", b"photo");
    let mut space = fx.space.clone();
    space.backup_marker_template = "{date}".into();
    fx.store.put(&space).unwrap();
    let mut destination = fx.destination.clone();
    destination.path_template = "{backup_folder}".into();
    destination.use_backup_marker = true;
    destination.subfolder_per_source = false;
    fx.store.put(&destination).unwrap();
    let (core, _t) = core(&fx);
    let context = crate::plan::WorkspaceContext {
        space_id: "space".into(),
        project_id: None,
    };
    assert!(core.workspace_status(&context).unwrap().flows[0].runnable);
    core.run_workspace_flow(&context, "flow").unwrap();
    wait_idle(&core);
    let folder = crate::paths::read_backup_folder(fx.card_dir.path()).unwrap();
    assert_eq!(
        fs::read(fx.nas_dir.path().join(folder).join("A.JPG")).unwrap(),
        b"photo"
    );
    assert_eq!(
        core.workspace_status(&context).unwrap().destinations[0].transferred,
        1
    );
}

#[test]
fn destination_kind_defaults_to_folder_for_old_snapshots() {
    let destination: Destination = serde_json::from_value(json!({
        "id": "dst",
        "space_id": "space",
        "device_id": "nas",
        "path_template": "photo",
        "app_path": "/old/local/path"
    }))
    .unwrap();
    assert_eq!(destination.kind, DestinationKind::Folder);
}

#[test]
fn app_destinations_are_excluded_from_run_all() {
    let fx = Fixture::new();
    fx.write_card_file("DCIM/A.JPG", b"a");
    let mut destination = fx.destination.clone();
    destination.kind = DestinationKind::App;
    destination.device_id.clear();
    destination.path_template.clear();
    destination.app_name = Some("Lightroom".into());
    destination.counts_as_safe_copy = false;
    fx.store.put(&destination).unwrap();
    let (core, _t) = core(&fx);
    assert!(core
        .run_flow("project", "flow")
        .unwrap_err()
        .contains("manually"));
    assert!(core.run_all("project").unwrap().is_empty());
}

#[test]
fn confirming_app_import_only_marks_current_to_transfer_files() {
    let fx = Fixture::new();
    fx.write_card_file("DCIM/A.JPG", b"a");
    fx.write_card_file("DCIM/B.MOV", b"bb");
    let mut destination = fx.destination.clone();
    destination.kind = DestinationKind::App;
    destination.device_id.clear();
    destination.path_template.clear();
    destination.app_name = Some("Lightroom".into());
    destination.counts_as_safe_copy = false;
    destination.rules = vec![crate::domain::FileRule::path(
        crate::domain::RuleAction::Include,
        crate::domain::RuleSyntax::Glob,
        "*.JPG",
    )];
    fx.store.put(&destination).unwrap();
    let (core, _t) = core(&fx);
    let err = core.prepare_app_import("project", "flow").unwrap_err();
    assert_eq!(err, "Choose the application for Lightroom on this computer");
    let app_path = fake_app(fx.card_dir.path());
    let mut settings = core.settings();
    settings.app_destinations.insert(
        destination.id.clone(),
        app_path.to_string_lossy().into_owned(),
    );
    core.save_settings(&settings).unwrap();
    let prepared = core.prepare_app_import("project", "flow").unwrap();
    assert_eq!(prepared.app_name, "Lightroom");
    assert_eq!(
        prepared.app_path,
        app_path
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .into_owned()
    );
    assert_eq!(prepared.paths.len(), 1);
    assert_eq!(prepared.files[0].rel_path, "A.JPG");

    destination.rules.clear();
    destination.rules.push(crate::domain::FileRule::path(
        crate::domain::RuleAction::Exclude,
        crate::domain::RuleSyntax::Glob,
        "*.JPG",
    ));
    fx.store.put(&destination).unwrap();
    assert_eq!(
        core.confirm_app_import("project", "flow", &prepared.token)
            .unwrap(),
        0
    );
    assert_eq!(
        core.list_files(&ListFilesRequest {
            project_id: "project".into(),
            flow_id: "flow".into(),
            category: Category::Transferred,
            offset: 0,
            limit: 50,
            filter: None,
            directory: None,
        })
        .unwrap()
        .total,
        0
    );
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

fn fake_app(dir: &std::path::Path) -> std::path::PathBuf {
    #[cfg(target_os = "macos")]
    {
        let path = dir.join("Lightroom.app");
        fs::create_dir_all(&path).unwrap();
        path
    }
    #[cfg(target_os = "windows")]
    {
        let path = dir.join("Lightroom.exe");
        fs::write(&path, b"").unwrap();
        path
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let path = dir.join("lightroom.desktop");
        fs::write(&path, b"").unwrap();
        path
    }
}
