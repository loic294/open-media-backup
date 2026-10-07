use super::*;
use crate::app::AppCore;
use crate::domain::{EntityKind, FileCopy, FileRecord, HashAlgo, RemoteHash, SafeCopyOverride};
use crate::plan::WorkspaceContext;
use crate::testing::{write, Fixture, MapResolver};
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn context(fx: &Fixture) -> WorkspaceContext {
    WorkspaceContext {
        space_id: fx.space.id.clone(),
        project_id: None,
    }
}

fn handle() -> JobHandle {
    JobHandle::new(
        TransferJob::new("check".into(), "dst".into(), String::new(), JobKind::Check),
        Arc::new(|| {}),
    )
}

fn scan(fx: &Fixture) -> CheckResults {
    let handle = handle();
    let destination = fx.store.get("dst").unwrap().unwrap();
    run_destination_check(&fx.store, &fx.resolver, &context(fx), &destination, &handle).unwrap();
    let snapshot = handle.snapshot();
    assert_eq!(snapshot.files_done, snapshot.files_total);
    snapshot.check_results.unwrap()
}

fn app(fx: &Fixture) -> AppCore {
    let resolver = Arc::new(MapResolver(Mutex::new(fx.resolver.0.lock().clone())));
    AppCore::new(
        fx.store.clone(),
        resolver,
        fx.nas_dir.path().join("thumbnails"),
        |_| {},
    )
}

fn wait(app: &AppCore) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while app.transfers.is_busy() {
        assert!(Instant::now() < deadline, "check did not finish");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(app
        .transfers
        .jobs()
        .iter()
        .all(|job| job.state == JobState::Done));
}

fn claim(fx: &Fixture, path: &str, bytes: &[u8], algo: HashAlgo) -> FileCopy {
    let mut hasher = omb_hash::hasher(algo);
    hasher.update(bytes);
    let hash = hasher.finish_hex();
    let file_id = FileRecord::id_for(algo, &hash);
    fx.store
        .put(&FileRecord {
            id: file_id.clone(),
            hash,
            hash_algo: algo,
            size: bytes.len() as u64,
            ..Default::default()
        })
        .unwrap();
    let copy = FileCopy {
        id: FileCopy::id_for(&file_id, "nas", path),
        file_id,
        device_id: "nas".into(),
        path: path.into(),
        ..Default::default()
    };
    fx.store.put(&copy).unwrap();
    copy
}

fn second_source(fx: &Fixture) {
    let mut source = fx.source.clone();
    source.id = "src2".into();
    source.path_template = "OTHER".into();
    fx.store.put(&source).unwrap();
    let mut flow = fx.flow.clone();
    flow.id = "flow2".into();
    flow.source_id = source.id;
    fx.store.put(&flow).unwrap();
    fx.write_card_file("OTHER/B.JPG", b"b");
}

/// Files directly in the destination template folder, without per-source subfolders.
fn flat(fx: &Fixture) {
    let mut destination: crate::domain::Destination = fx.store.get("dst").unwrap().unwrap();
    destination.subfolder_per_source = false;
    fx.store.put(&destination).unwrap();
}

fn selected(ids: &[&str]) -> CheckScope {
    CheckScope::SelectedSources {
        source_ids: ids.iter().map(|id| (*id).into()).collect(),
    }
}

#[test]
fn check_scope_serialization_is_tagged_and_defaults_to_configured_sources() {
    assert_eq!(
        serde_json::to_value(CheckScope::default()).unwrap(),
        serde_json::json!({"kind": "configuredSources"})
    );
    assert_eq!(
        serde_json::from_value::<CheckScope>(
            serde_json::json!({"kind":"selectedSources", "sourceIds":["src"]})
        )
        .unwrap(),
        selected(&["src"])
    );
    assert_eq!(
        serde_json::to_value(CheckScope::AllDestination).unwrap(),
        serde_json::json!({"kind":"allDestination"})
    );
    assert!(
        serde_json::from_value::<CheckScope>(serde_json::json!({"kind":"selectedSources"}))
            .is_err()
    );
    assert!(serde_json::from_value::<CheckScope>(serde_json::json!({"kind":"unknown"})).is_err());
    let results = serde_json::to_value(CheckResults::default()).unwrap();
    assert_eq!(results["verified"], 0);
    assert_eq!(results["untracked"], 0);
}

#[test]
fn check_scope_selected_sources_filters_and_deduplicates_before_enqueue() {
    let fx = Fixture::new();
    fx.write_card_file("DCIM/A.JPG", b"a");
    second_source(&fx);
    let app = app(&fx);
    let ids = app
        .check_workspace_destination_scoped(&context(&fx), "dst", selected(&["src2", "src2"]))
        .unwrap();
    assert_eq!(ids.len(), 1);
    wait(&app);
    let jobs = app.transfers.jobs();
    assert_eq!(jobs[0].flow_id, "check:flow2");
    let results = jobs[0].check_results.as_ref().unwrap();
    assert_eq!(results.missing, 1);
    assert_eq!(results.items[0].source_path, "B.JPG");
}

#[test]
fn check_scope_rejects_empty_stale_foreign_and_invalid_sources_atomically() {
    for invalid in ["", "stale", "foreign", "src2"] {
        let fx = Fixture::new();
        fx.write_card_file("DCIM/A.JPG", b"a");
        second_source(&fx);
        let mut source = fx.source.clone();
        source.id = "foreign".into();
        source.space_id = "other-space".into();
        fx.store.put(&source).unwrap();
        if invalid == "src2" {
            let mut source: crate::domain::Source = fx.store.get("src2").unwrap().unwrap();
            source.path_template = "{unavailable}".into();
            fx.store.put(&source).unwrap();
        }
        let app = app(&fx);
        let scope = if invalid.is_empty() {
            selected(&[])
        } else {
            selected(&["src", invalid])
        };
        assert!(
            app.check_workspace_destination_scoped(&context(&fx), "dst", scope)
                .is_err(),
            "{invalid}"
        );
        assert!(
            app.transfers.jobs().is_empty(),
            "partially enqueued {invalid}"
        );
    }
}

#[test]
fn check_scope_rejects_source_without_incoming_flow_and_disconnected_source() {
    let fx = Fixture::new();
    second_source(&fx);
    fx.store.delete(EntityKind::Flow, "flow2").unwrap();
    let app = app(&fx);
    assert!(app
        .check_workspace_destination_scoped(&context(&fx), "dst", selected(&["src", "src2"]))
        .is_err());
    assert!(app.transfers.jobs().is_empty());
    fx.unmount("card");
    let disconnected = self::app(&fx);
    assert!(disconnected
        .check_workspace_destination_scoped(&context(&fx), "dst", selected(&["src"]))
        .is_err());
    assert!(disconnected.transfers.jobs().is_empty());
}

#[test]
fn check_scope_configured_sources_wrapper_preserves_valid_incoming_checks() {
    let fx = Fixture::new();
    fx.write_card_file("DCIM/A.JPG", b"a");
    second_source(&fx);
    let mut source: crate::domain::Source = fx.store.get("src2").unwrap().unwrap();
    source.device_id = "offline".into();
    let mut device = fx.card.clone();
    device.id = "offline".into();
    fx.store.put(&device).unwrap();
    fx.store.put(&source).unwrap();
    let app = app(&fx);
    let ids = app
        .check_workspace_destination(&context(&fx), "dst")
        .unwrap();
    assert_eq!(ids.len(), 1);
    wait(&app);
    assert_eq!(
        app.transfers.jobs()[0]
            .check_results
            .as_ref()
            .unwrap()
            .missing,
        1
    );
}

#[test]
fn check_scope_all_destination_works_without_sources_or_flows() {
    let fx = Fixture::new();
    flat(&fx);
    fx.store.delete(EntityKind::Flow, "flow").unwrap();
    fx.unmount("card");
    write(fx.nas_dir.path(), "photo/Trip/old-source/A.JPG", b"a");
    let app = app(&fx);
    let ids = app
        .check_workspace_destination_scoped(&context(&fx), "dst", CheckScope::AllDestination)
        .unwrap();
    assert_eq!(ids.len(), 1);
    wait(&app);
    let results = app.transfers.jobs()[0].check_results.clone().unwrap();
    assert_eq!(results.untracked, 1);
    assert_eq!(results.matched + results.verified, 0);
    assert!(results.items[0].source_path.is_empty());
}

#[test]
fn check_scope_full_scan_verifies_recorded_algorithms_and_inventories_missing_changed_untracked() {
    let fx = Fixture::new();
    flat(&fx);
    fx.unmount("card");
    for (name, algo) in [("A.JPG", HashAlgo::Blake3), ("B.JPG", HashAlgo::Xxh64)] {
        let path = format!("photo/Trip/old/{name}");
        write(fx.nas_dir.path(), &path, b"same");
        claim(&fx, &path, b"same", algo);
    }
    let changed = claim(&fx, "photo/Trip/changed.JPG", b"before", HashAlgo::Xxh64);
    write(fx.nas_dir.path(), &changed.path, b"after!");
    let missing = claim(&fx, "photo/Trip/missing.JPG", b"missing", HashAlgo::Blake3);
    let outside = claim(&fx, "other/missing.JPG", b"outside", HashAlgo::Blake3);
    write(fx.nas_dir.path(), "photo/Trip/untracked.JPG", b"new");
    write(fx.nas_dir.path(), "other/untracked.JPG", b"outside");
    let results = scan(&fx);
    assert_eq!(
        (
            results.verified,
            results.untracked,
            results.conflicts,
            results.missing,
            results.errors
        ),
        (2, 1, 1, 1, 0)
    );
    assert_eq!(results.matched, 0);
    assert_eq!(results.items.len(), 3);
    assert!(results.items.iter().all(|item| item.source_path.is_empty()));
    assert!(
        fx.store
            .get::<FileCopy>(&changed.id)
            .unwrap()
            .unwrap()
            .removed
    );
    assert!(
        fx.store
            .get::<FileCopy>(&missing.id)
            .unwrap()
            .unwrap()
            .removed
    );
    assert!(
        !fx.store
            .get::<FileCopy>(&outside.id)
            .unwrap()
            .unwrap()
            .removed
    );
    assert_eq!(
        std::fs::read(fx.nas_dir.path().join(&changed.path)).unwrap(),
        b"after!"
    );
    assert_eq!(fx.store.list::<FileRecord>().unwrap().len(), 5);
    assert!(!fx
        .card_dir
        .path()
        .join(crate::paths::BACKUP_MARKER_FILE)
        .exists());
}

#[test]
fn check_scope_full_scan_only_scans_existing_project_and_catalog_folders() {
    let fx = Fixture::new();
    fx.unmount("card");
    let mut destination = fx.destination.clone();
    destination.path_template = "photo/{project}/{backup_folder}".into();
    destination.use_backup_marker = true;
    fx.store.put(&destination).unwrap();
    // Card name subfolder of a backup folder recorded in the catalog.
    let known = claim(
        &fx,
        "photo/Trip/2026-01-01/Camera A Card 1/A.JPG",
        b"a",
        HashAlgo::Xxh64,
    );
    write(fx.nas_dir.path(), &known.path, b"a");
    write(
        fx.nas_dir.path(),
        "photo/Trip/2026-01-01/Camera A Card 1/new.JPG",
        b"n",
    );
    // Not a recorded backup folder, another source's folder, an unknown project.
    write(
        fx.nas_dir.path(),
        "photo/Trip/2026-02-02/Camera A Card 1/x",
        b"x",
    );
    write(
        fx.nas_dir.path(),
        "photo/Trip/2026-01-01/Other card/y",
        b"y",
    );
    write(
        fx.nas_dir.path(),
        "photo/Old/2026-01-01/Camera A Card 1/z",
        b"z",
    );
    write(fx.nas_dir.path(), "unrelated/three", b"3");
    let results = scan(&fx);
    assert_eq!((results.verified, results.untracked), (1, 1));
    assert_eq!(results.missing + results.errors, 0);
    // A connected card's marker adds its current backup folder.
    fx.resolver
        .0
        .lock()
        .insert("card".into(), fx.card_dir.path().to_path_buf());
    crate::paths::ensure_backup_folder(fx.card_dir.path(), "2026-02-02").unwrap();
    assert_eq!(scan(&fx).untracked, 2);
}

#[test]
fn check_scope_full_scan_reports_nothing_without_known_folders() {
    let fx = Fixture::new();
    fx.unmount("card");
    let mut destination = fx.destination.clone();
    destination.path_template = "{backup_folder}".into();
    destination.use_backup_marker = true;
    fx.store.put(&destination).unwrap();
    write(fx.nas_dir.path(), "2026-01-01/Camera A Card 1/A.JPG", b"a");
    let handle = handle();
    run_destination_check(
        &fx.store,
        &fx.resolver,
        &context(&fx),
        &destination,
        &handle,
    )
    .unwrap();
    let job = handle.snapshot();
    assert_eq!(job.check_results.unwrap().items.len(), 0);
    assert_eq!(job.warnings.len(), 1, "{:?}", job.warnings);
}

#[test]
fn check_scope_full_scan_never_fabricates_pass_for_unusable_catalog_evidence() {
    let fx = Fixture::new();
    flat(&fx);
    let copy = claim(&fx, "photo/Trip/bad.JPG", b"same", HashAlgo::Blake3);
    write(fx.nas_dir.path(), &copy.path, b"different");
    fx.store
        .delete(EntityKind::FileRecord, &copy.file_id)
        .unwrap();
    let results = scan(&fx);
    assert_eq!(
        (results.untracked, results.verified, results.conflicts),
        (1, 0, 0)
    );
    assert!(!fx.store.get::<FileCopy>(&copy.id).unwrap().unwrap().removed);
}

#[test]
fn check_scope_full_scan_assesses_every_live_claim_at_the_same_location() {
    let fx = Fixture::new();
    flat(&fx);
    let path = "photo/Trip/A.JPG";
    let stale = claim(&fx, path, b"old", HashAlgo::Blake3);
    let current = claim(&fx, path, b"new", HashAlgo::Xxh64);
    write(fx.nas_dir.path(), path, b"new");
    assert_eq!(scan(&fx).conflicts, 1);
    assert!(
        fx.store
            .get::<FileCopy>(&stale.id)
            .unwrap()
            .unwrap()
            .removed
    );
    assert!(
        !fx.store
            .get::<FileCopy>(&current.id)
            .unwrap()
            .unwrap()
            .removed
    );
    assert_eq!(scan(&fx).verified, 1);
}

#[test]
fn check_scope_full_scan_reports_unsafe_claim_paths_without_invalidating_or_reading_them() {
    let fx = Fixture::new();
    flat(&fx);
    let unsafe_copy = claim(
        &fx,
        "photo/Trip/../../secret.JPG",
        b"secret",
        HashAlgo::Blake3,
    );
    fx.write_card_file("secret.JPG", b"secret");
    let results = scan(&fx);
    assert_eq!(results.errors, 1);
    assert_eq!(results.verified + results.missing + results.untracked, 0);
    assert!(
        !fx.store
            .get::<FileCopy>(&unsafe_copy.id)
            .unwrap()
            .unwrap()
            .removed
    );
}

#[test]
fn check_scope_rejects_invalid_destination_and_workspace_before_enqueue() {
    let fx = Fixture::new();
    let app = app(&fx);
    let wrong_context = WorkspaceContext {
        space_id: "foreign".into(),
        project_id: None,
    };
    for scope in [
        CheckScope::ConfiguredSources,
        selected(&["src"]),
        CheckScope::AllDestination,
    ] {
        assert!(app
            .check_workspace_destination_scoped(&wrong_context, "dst", scope.clone())
            .is_err());
        assert!(app
            .check_workspace_destination_scoped(&context(&fx), "missing", scope)
            .is_err());
    }
    let mut destination = fx.destination.clone();
    destination.kind = crate::domain::DestinationKind::App;
    fx.store.put(&destination).unwrap();
    assert!(app
        .check_workspace_destination_scoped(&context(&fx), "dst", CheckScope::AllDestination)
        .is_err());
    assert!(app.transfers.jobs().is_empty());
    destination.kind = crate::domain::DestinationKind::Folder;
    destination.path_template = "../outside".into();
    fx.store.put(&destination).unwrap();
    assert!(app
        .check_workspace_destination_scoped(&context(&fx), "dst", CheckScope::AllDestination)
        .is_err());
    assert!(app.transfers.jobs().is_empty());
}

#[test]
fn check_scope_full_scan_checks_acknowledged_destination_hash_not_source_hash() {
    let fx = Fixture::new();
    flat(&fx);
    let copy = claim(&fx, "photo/Trip/skipped.JPG", b"source", HashAlgo::Xxh64);
    let mut destination_hash = omb_hash::hasher(HashAlgo::Xxh64);
    destination_hash.update(b"destination");
    let override_ = SafeCopyOverride {
        id: "override".into(),
        space_id: fx.space.id.clone(),
        file_id: copy.file_id.clone(),
        destination_device_id: "nas".into(),
        destination_path: copy.path.clone(),
        destination_hash: destination_hash.finish_hex(),
        ..Default::default()
    };
    fx.store.put(&override_).unwrap();
    write(fx.nas_dir.path(), &copy.path, b"destination");
    // The source-content copy claim is stale, but the independently verified
    // acknowledgement must survive that invalidation.
    assert_eq!(scan(&fx).conflicts, 1);
    assert!(fx
        .store
        .get::<SafeCopyOverride>(&override_.id)
        .unwrap()
        .is_some());
    assert_eq!(scan(&fx).verified, 1);
    write(fx.nas_dir.path(), &copy.path, b"edited");
    assert_eq!(scan(&fx).conflicts, 1);
    assert!(fx
        .store
        .get::<SafeCopyOverride>(&override_.id)
        .unwrap()
        .is_none());
}

#[cfg(unix)]
#[test]
fn check_scope_full_scan_does_not_follow_file_directory_or_root_symlinks() {
    use std::os::unix::fs::symlink;
    let fx = Fixture::new();
    flat(&fx);
    let external = fx.write_card_file("external/secret.JPG", b"secret");
    std::fs::create_dir_all(fx.nas_dir.path().join("photo/Trip")).unwrap();
    symlink(
        &external,
        fx.nas_dir.path().join("photo/Trip/file-link.JPG"),
    )
    .unwrap();
    symlink(
        external.parent().unwrap(),
        fx.nas_dir.path().join("photo/Trip/dir-link"),
    )
    .unwrap();
    let linked = claim(
        &fx,
        "photo/Trip/dir-link/secret.JPG",
        b"secret",
        HashAlgo::Blake3,
    );
    let missing_under_link = claim(
        &fx,
        "photo/Trip/dir-link/missing.JPG",
        b"gone",
        HashAlgo::Blake3,
    );
    symlink(
        fx.card_dir.path().join("does-not-exist"),
        fx.nas_dir.path().join("photo/Trip/broken-link"),
    )
    .unwrap();
    let results = scan(&fx);
    assert_eq!(results.errors, 5);
    assert_eq!(
        results.verified + results.matched + results.untracked + results.missing,
        0
    );
    for claim in [linked, missing_under_link] {
        assert!(
            !fx.store
                .get::<FileCopy>(&claim.id)
                .unwrap()
                .unwrap()
                .removed
        );
    }
    assert_eq!(std::fs::read(&external).unwrap(), b"secret");
    let mut destination = fx.destination.clone();
    destination.path_template = "photo/Trip/dir-link".into();
    fx.store.put(&destination).unwrap();
    assert!(scan(&fx).errors > 0);
}

#[test]
fn check_scope_full_scan_uses_remote_hasher_for_catalog_algorithm() {
    use crate::hash_server::{HashRequest, HashResponse};
    use axum::{routing::post, Json, Router};
    use std::sync::atomic::{AtomicUsize, Ordering};
    let fx = Fixture::new();
    flat(&fx);
    let copy = claim(&fx, "photo/Trip/A.JPG", b"same", HashAlgo::Xxh64);
    write(fx.nas_dir.path(), &copy.path, b"same");
    let record = fx.store.get::<FileRecord>(&copy.file_id).unwrap().unwrap();
    let requests = Arc::new(AtomicUsize::new(0));
    let count = requests.clone();
    let server = crate::testing::HttpServer::start(Router::new().route(
        "/v1/hash",
        post(move |Json(request): Json<HashRequest>| {
            let count = count.clone();
            let hash = record.hash.clone();
            async move {
                assert_eq!(request.algo, HashAlgo::Xxh64);
                assert_eq!(request.rel_path, "photo/Trip/A.JPG");
                count.fetch_add(1, Ordering::SeqCst);
                Json(HashResponse {
                    hash,
                    size: 4,
                    modified: None,
                })
            }
        }),
    ));
    fx.store
        .save_hash_server(&crate::store::HashServer {
            id: "remote".into(),
            address: server.address.clone(),
            token: "token".into(),
            ..Default::default()
        })
        .unwrap();
    let mut destination: crate::domain::Destination = fx.store.get("dst").unwrap().unwrap();
    destination.remote_hash = Some(RemoteHash {
        server_id: "remote".into(),
        root: "a".repeat(64),
        enabled: true,
    });
    fx.store.put(&destination).unwrap();
    let handle = handle();
    run_destination_check(
        &fx.store,
        &fx.resolver,
        &context(&fx),
        &destination,
        &handle,
    )
    .unwrap();
    assert_eq!(requests.load(Ordering::SeqCst), 1);
    let job = handle.snapshot();
    assert!(job.remote_hash_active);
    assert!(job.warnings.is_empty(), "{:?}", job.warnings);
    assert_eq!(job.check_results.unwrap().verified, 1);
}

#[test]
fn check_scope_source_checks_skip_offline_sources_with_a_warning() {
    for scope in [CheckScope::ConfiguredSources, selected(&["src", "src2"])] {
        let fx = Fixture::new();
        fx.write_card_file("DCIM/A.JPG", b"a");
        second_source(&fx);
        let mut source: crate::domain::Source = fx.store.get("src2").unwrap().unwrap();
        source.device_id = "offline".into();
        source.task_name = "Offline card".into();
        let mut device = fx.card.clone();
        device.id = "offline".into();
        fx.store.put(&device).unwrap();
        fx.store.put(&source).unwrap();
        let app = app(&fx);
        let ids = app
            .check_workspace_destination_scoped(&context(&fx), "dst", scope.clone())
            .unwrap();
        assert_eq!(ids.len(), 1, "{scope:?}");
        wait(&app);
        let job = &app.transfers.jobs()[0];
        assert_eq!(job.flow_id, "check:flow");
        assert_eq!(job.warnings, vec!["Skipped offline sources: Offline card"]);
    }
}

#[test]
fn check_scope_source_checks_fail_when_every_source_is_offline() {
    let fx = Fixture::new();
    fx.unmount("card");
    let app = app(&fx);
    for scope in [CheckScope::ConfiguredSources, selected(&["src"])] {
        let error = app
            .check_workspace_destination_scoped(&context(&fx), "dst", scope)
            .unwrap_err();
        assert!(error.starts_with("Connect a source device"), "{error}");
    }
    assert!(app.transfers.jobs().is_empty());
}
