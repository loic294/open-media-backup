use super::*;
use crate::domain::{FileCopy, VerifyMode};
use crate::plan::{classify_flow, Catalog, Category, FailureMap};
use crate::testing::{write, Fixture};
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn handle() -> JobHandle {
    JobHandle::new(
        TransferJob {
            analysis: None,
            id: "j".into(),
            flow_id: "flow".into(),
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

fn run(fx: &Fixture) -> (JobHandle, Mutex<FailureMap>) {
    let (h, failures) = (handle(), Mutex::new(FailureMap::new()));
    run_transfer(
        &fx.store,
        &fx.resolver,
        &fx.project.id,
        &fx.flow.id,
        &h,
        &failures,
    )
    .unwrap();
    (h, failures)
}

fn categories(fx: &Fixture) -> Vec<(String, Category)> {
    let ctx =
        crate::plan::resolve_flow(&fx.store, &fx.resolver, &fx.project.id, &fx.flow.id).unwrap();
    let catalog = Catalog::load(&fx.store).unwrap();
    classify_flow(&ctx, &catalog, None)
        .into_iter()
        .map(|f| (f.rel_path, f.category))
        .collect()
}

#[test]
fn copies_files_and_records_lineage() {
    let fx = Fixture::new();
    fx.write_card_file("DCIM/100/A.JPG", b"photo a");
    fx.write_card_file("DCIM/100/B.MP4", b"video b");
    let (h, _) = run(&fx);
    let job = h.snapshot();
    assert_eq!(
        (job.files_done, job.files_total, job.bytes_done),
        (2, 2, 14)
    );
    let target = fx
        .nas_dir
        .path()
        .join("photo/Trip/Camera A Card 1/100/A.JPG");
    assert_eq!(std::fs::read(target).unwrap(), b"photo a");
    let copies = fx.store.list::<FileCopy>().unwrap();
    assert_eq!(copies.len(), 4);
    assert!(copies
        .iter()
        .any(|c| c.device_id == "card" && c.path == "DCIM/100/A.JPG"));
    assert!(categories(&fx)
        .iter()
        .all(|(_, c)| *c == Category::Transferred));
}

#[test]
fn second_run_is_a_no_op() {
    let fx = Fixture::new();
    fx.write_card_file("DCIM/A.JPG", b"a");
    run(&fx);
    let (h, _) = run(&fx);
    assert_eq!(h.snapshot().files_total, 0);
}

#[test]
fn preserves_source_relative_folders_and_same_names_without_copying_empty_or_excluded_folders() {
    for per_source in [false, true] {
        let fx = Fixture::new();
        let mut destination = fx.destination.clone();
        destination.path_template = "backup".into();
        destination.subfolder_per_source = per_source;
        destination.rules = vec![crate::domain::FileRule::path(
            crate::domain::RuleAction::Exclude,
            crate::domain::RuleSyntax::Glob,
            "*.TXT",
        )];
        fx.store.put(&destination).unwrap();
        for (path, bytes) in [
            ("ROOT.JPG", b"root".as_slice()),
            ("100/A.JPG", b"first".as_slice()),
            ("101/nested/A.JPG", b"second".as_slice()),
        ] {
            fx.write_card_file(&format!("DCIM/{path}"), bytes);
        }
        fx.write_card_file("DCIM/excluded/NOTES.TXT", b"excluded");
        std::fs::create_dir_all(fx.card_dir.path().join("DCIM/empty")).unwrap();
        let (handle, failures) = run(&fx);
        assert!(handle.snapshot().errors.is_empty());
        assert!(failures.lock().is_empty());
        assert_eq!(handle.snapshot().files_total, 3);
        let base = if per_source {
            "backup/Camera A Card 1"
        } else {
            "backup"
        };
        for (path, bytes) in [
            ("ROOT.JPG", b"root".as_slice()),
            ("100/A.JPG", b"first".as_slice()),
            ("101/nested/A.JPG", b"second".as_slice()),
        ] {
            assert_eq!(
                std::fs::read(fx.nas_dir.path().join(format!("{base}/{path}"))).unwrap(),
                bytes
            );
            assert!(fx.card_dir.path().join(format!("DCIM/{path}")).exists());
        }
        for absent in ["DCIM", "empty", "excluded", "A.JPG"] {
            assert!(!fx.nas_dir.path().join(format!("{base}/{absent}")).exists());
        }
        let copies = fx.store.list::<FileCopy>().unwrap();
        assert!(
            copies
                .iter()
                .any(|copy| copy.device_id == "nas"
                    && copy.path == format!("{base}/101/nested/A.JPG"))
        );
        assert_eq!(run(&fx).0.snapshot().files_total, 0);
    }
}

/// Synthetic TIFF containing only DateTimeOriginal and OffsetTimeOriginal.
fn capture_fixture(offset: bool) -> Vec<u8> {
    let mut bytes = vec![0; 83];
    bytes[..8].copy_from_slice(&[b'I', b'I', 42, 0, 8, 0, 0, 0]);
    bytes[8..10].copy_from_slice(&1u16.to_le_bytes());
    let mut entry = |position: usize, tag: u16, kind: u16, count: u32, value: u32| {
        bytes[position..position + 2].copy_from_slice(&tag.to_le_bytes());
        bytes[position + 2..position + 4].copy_from_slice(&kind.to_le_bytes());
        bytes[position + 4..position + 8].copy_from_slice(&count.to_le_bytes());
        bytes[position + 8..position + 12].copy_from_slice(&value.to_le_bytes());
    };
    entry(10, 0x8769, 4, 1, 26);
    entry(28, 0x9003, 2, 20, 56);
    if offset {
        entry(40, 0x9011, 2, 7, 76);
    }
    bytes[26..28].copy_from_slice(&(if offset { 2u16 } else { 1u16 }).to_le_bytes());
    bytes[56..76].copy_from_slice(b"2026:01:01 12:00:00\0");
    bytes[76..83].copy_from_slice(b"+00:00\0");
    bytes
}

#[test]
fn overlapping_projects_transfer_each_route_preserving_physical_source_path() {
    let fx = Fixture::new();
    let bytes = capture_fixture(true);
    fx.write_card_file("DCIM/A.DNG", &bytes);
    let instant = chrono::DateTime::parse_from_rfc3339("2026-01-01T12:00:00Z")
        .unwrap()
        .timestamp_millis();
    for (id, name) in [("a", "Alice"), ("b", "Bob")] {
        fx.store
            .put(&crate::domain::Project {
                id: id.into(),
                name: name.into(),
                space_id: fx.space.id.clone(),
                start_time: Some(instant),
                end_time: Some(instant),
                values: std::collections::BTreeMap::from([("client".into(), name.into())]),
                ..Default::default()
            })
            .unwrap();
    }
    let mut destination = fx.destination.clone();
    destination.path_template = "{client}/{project_name}".into();
    destination.subfolder_per_source = false;
    fx.store.put(&destination).unwrap();
    write(
        fx.nas_dir.path(),
        "Bob/Bob/A.DNG",
        b"existing unrelated file",
    );
    let thumbnails = tempfile::tempdir().unwrap();
    let app = crate::app::AppCore::new(
        fx.store.clone(),
        Arc::new(crate::testing::MapResolver(Mutex::new(
            fx.resolver.0.lock().clone(),
        ))),
        thumbnails.path().into(),
        |_| {},
    );
    let page = app
        .list_files(&crate::app::ListFilesRequest {
            project_id: fx.project.id.clone(),
            flow_id: fx.flow.id.clone(),
            category: Category::ToTransfer,
            offset: 0,
            limit: 50,
            filter: None,
            directory: None,
        })
        .unwrap();
    assert_eq!(page.items.len(), 2);
    assert_eq!(
        page.directories
            .iter()
            .find(|directory| directory.path.is_empty())
            .unwrap()
            .total,
        2
    );
    for folder in ["Alice/Alice", "Bob/Bob"] {
        assert!(page
            .directories
            .iter()
            .any(|directory| directory.path == folder && directory.direct_files == 1));
    }
    assert!(page
        .items
        .iter()
        .all(|file| file.rel_path == "A.DNG" && file.capture_time == Some(instant)));
    assert!(page
        .items
        .iter()
        .any(|file| file.project_id.as_deref() == Some("a")
            && file.target_path.as_deref() == Some("Alice/Alice/A.DNG")));
    assert!(page
        .items
        .iter()
        .any(|file| file.project_id.as_deref() == Some("b")
            && file.target_path.as_deref() == Some("Bob/Bob/A.DNG")));
    let (handle, _) = run(&fx);
    assert_eq!(handle.snapshot().files_total, 2);
    for target in ["Alice/Alice/A.DNG", "Bob/Bob/A (1).DNG"] {
        assert_eq!(
            std::fs::read(fx.nas_dir.path().join(target)).unwrap(),
            bytes
        );
    }
    assert_eq!(
        std::fs::read(fx.nas_dir.path().join("Bob/Bob/A.DNG")).unwrap(),
        b"existing unrelated file"
    );
    let copies = fx.store.list::<FileCopy>().unwrap();
    assert_eq!(copies.len(), 3);
    assert_eq!(
        copies
            .iter()
            .filter(|copy| copy.device_id == "card")
            .count(),
        1
    );
    assert!(copies
        .iter()
        .any(|copy| copy.device_id == "card" && copy.path == "DCIM/A.DNG"));
    let records = fx.store.list::<crate::domain::FileRecord>().unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].origin_path, "DCIM/A.DNG");
    assert_eq!(run(&fx).0.snapshot().files_total, 0);
    assert!(categories(&fx)
        .iter()
        .all(|(_, category)| *category == Category::Transferred));
}

#[test]
fn unknown_timezone_capture_is_not_transferred_through_project_variables() {
    let fx = Fixture::new();
    fx.write_card_file("DCIM/A.DNG", &capture_fixture(false));
    let mut project = fx.project.clone();
    project.start_time = Some(0);
    project.end_time = Some(2_000_000_000_000);
    fx.store.put(&project).unwrap();
    let mut destination = fx.destination.clone();
    destination.path_template = "{project_name}".into();
    fx.store.put(&destination).unwrap();
    assert_eq!(run(&fx).0.snapshot().files_total, 0);
    assert_eq!(categories(&fx)[0].1, Category::Ignored);
    assert!(fx.store.list::<FileCopy>().unwrap().is_empty());
}

#[test]
fn adopts_identical_and_renames_conflicting_targets() {
    let fx = Fixture::new();
    fx.write_card_file("DCIM/A.JPG", b"same");
    fx.write_card_file("DCIM/B.JPG", b"new content");
    let dest = fx.nas_dir.path().join("photo/Trip/Camera A Card 1");
    write(&dest, "A.JPG", b"same");
    write(&dest, "B.JPG", b"other file");
    run(&fx);
    assert_eq!(std::fs::read(dest.join("B.JPG")).unwrap(), b"other file");
    assert_eq!(
        std::fs::read(dest.join("B (1).JPG")).unwrap(),
        b"new content"
    );
    assert!(!dest.join("A (1).JPG").exists());
}

#[test]
fn reread_verification_and_no_partials_left() {
    let mut fx = Fixture::new();
    fx.space.verify_mode = VerifyMode::Reread;
    fx.store.put(&fx.space).unwrap();
    fx.write_card_file("DCIM/A.JPG", &vec![7u8; 9_000_000]);
    run(&fx);
    let dest = fx.nas_dir.path().join("photo/Trip/Camera A Card 1");
    let names: Vec<_> = std::fs::read_dir(&dest)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(names, vec!["A.JPG"]);
}

#[test]
fn backup_marker_folder_is_created_and_reused() {
    let mut fx = Fixture::new();
    fx.destination.use_backup_marker = true;
    fx.destination.subfolder_per_source = false;
    fx.destination.path_template = "backups/{backup_folder}".into();
    fx.store.put(&fx.destination).unwrap();
    fx.space.backup_marker_template = "card-{project_name}".into();
    fx.store.put(&fx.space).unwrap();
    fx.write_card_file("DCIM/A.JPG", b"a");
    run(&fx);
    assert!(fx.nas_dir.path().join("backups/card-Trip/A.JPG").exists());
    assert_eq!(
        crate::paths::read_backup_folder(fx.card_dir.path()).as_deref(),
        Some("card-Trip")
    );
}

#[test]
fn transfer_marker_uses_the_preview_route_date_not_the_workspace_date() {
    for project_override in [false, true] {
        for selected_project in [false, true] {
            let mut fx = Fixture::new();
            let bytes = capture_fixture(true);
            fx.write_card_file("DCIM/A.DNG", &bytes);
            let instant = chrono::DateTime::parse_from_rfc3339("2026-01-01T12:00:00Z")
                .unwrap()
                .timestamp_millis();
            fx.project.start_time = Some(instant);
            fx.project.end_time = Some(instant);
            fx.space.backup_marker_template = "{date}".into();
            fx.space.variables.push(crate::domain::VariableDef {
                name: "date".into(),
                default_value: if project_override {
                    "1999-01-01"
                } else {
                    "2026-10-06"
                }
                .into(),
                ..Default::default()
            });
            if project_override {
                fx.project.values.insert("date".into(), "2026-10-06".into());
            }
            fx.source.backup_name = "Sony A7 IV".into();
            fx.destination.path_template = "{backup_folder}".into();
            fx.destination.use_backup_marker = true;
            fx.destination.subfolder_per_source = true;
            fx.store.put(&fx.project).unwrap();
            fx.store.put(&fx.space).unwrap();
            fx.store.put(&fx.source).unwrap();
            fx.store.put(&fx.destination).unwrap();
            let unrelated = crate::domain::Project {
                id: "unrelated".into(),
                name: "Unrelated selection".into(),
                space_id: fx.space.id.clone(),
                values: std::collections::BTreeMap::from([("date".into(), "1999-01-01".into())]),
                ..Default::default()
            };
            fx.store.put(&unrelated).unwrap();
            let context = crate::plan::WorkspaceContext {
                space_id: fx.space.id.clone(),
                project_id: selected_project.then(|| unrelated.id.clone()),
            };
            let preview = crate::plan::workspace_status(
                &fx.store,
                &fx.resolver,
                &Catalog::load(&fx.store).unwrap(),
                &context,
                &FailureMap::new(),
            )
            .unwrap();
            assert_eq!(
                preview.destinations[0].path_previews[0].path,
                "2026-10-06/Sony A7 IV"
            );
            assert!(crate::paths::read_backup_folder(fx.card_dir.path()).is_none());
            let h = handle();
            run_workspace_transfer(
                &fx.store,
                &fx.resolver,
                &context,
                &fx.flow.id,
                &h,
                &Mutex::new(FailureMap::new()),
            )
            .unwrap();
            assert_eq!(h.snapshot().files_done, 1);
            assert_eq!(
                std::fs::read(fx.nas_dir.path().join("2026-10-06/Sony A7 IV/A.DNG")).unwrap(),
                bytes,
            );
            assert_eq!(
                crate::paths::read_backup_folder(fx.card_dir.path()).as_deref(),
                Some("2026-10-06"),
            );
            assert!(fx.store.list::<FileCopy>().unwrap().iter().any(|copy| {
                copy.device_id == fx.nas.id && copy.path == "2026-10-06/Sony A7 IV/A.DNG"
            }));
            assert_eq!(std::fs::read_dir(fx.nas_dir.path()).unwrap().count(), 1);
        }
    }
}

#[test]
fn conflicting_route_markers_fail_before_writing_marker_or_media() {
    let mut fx = Fixture::new();
    fx.write_card_file("DCIM/A.DNG", &capture_fixture(true));
    let instant = chrono::DateTime::parse_from_rfc3339("2026-01-01T12:00:00Z")
        .unwrap()
        .timestamp_millis();
    fx.space.allow_project_overlap = true;
    fx.space.backup_marker_template = "{date}".into();
    fx.store.put(&fx.space).unwrap();
    for (id, date) in [("a", "2026-10-06"), ("b", "2026-10-07")] {
        fx.store
            .put(&crate::domain::Project {
                id: id.into(),
                name: id.into(),
                space_id: fx.space.id.clone(),
                start_time: Some(instant),
                end_time: Some(instant),
                values: std::collections::BTreeMap::from([("date".into(), date.into())]),
                ..Default::default()
            })
            .unwrap();
    }
    fx.destination.use_backup_marker = true;
    fx.destination.path_template = "{backup_folder}".into();
    fx.store.put(&fx.destination).unwrap();
    let context = crate::plan::WorkspaceContext {
        space_id: fx.space.id.clone(),
        project_id: None,
    };
    let error = run_workspace_transfer(
        &fx.store,
        &fx.resolver,
        &context,
        &fx.flow.id,
        &handle(),
        &Mutex::new(FailureMap::new()),
    )
    .unwrap_err();
    assert!(error.contains("multiple folders"), "{error}");
    assert!(crate::paths::read_backup_folder(fx.card_dir.path()).is_none());
    assert_eq!(std::fs::read_dir(fx.nas_dir.path()).unwrap().count(), 0);
    assert!(fx.store.list::<FileCopy>().unwrap().is_empty());
}

#[test]
fn offline_destination_fails_cleanly() {
    let fx = Fixture::new();
    fx.unmount("nas");
    let err = run_transfer(
        &fx.store,
        &fx.resolver,
        "project",
        "flow",
        &handle(),
        &Mutex::new(FailureMap::new()),
    );
    assert!(err.unwrap_err().contains("Home NAS"));
}

#[test]
fn manager_runs_pauses_and_reports() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = seen.clone();
    let manager = TransferManager::new(move |jobs| *sink.lock() = jobs);
    let id = manager.enqueue(JobSpec {
        key: "k".into(),
        label: "test".into(),
        resources: vec![],
        kind: JobKind::Transfer,
        queue: None,
        work: Box::new(|h| {
            for _ in 0..5 {
                h.checkpoint().map_err(|_| "cancelled".to_string())?;
                std::thread::sleep(Duration::from_millis(20));
            }
            Ok(())
        }),
    });
    assert_eq!(
        manager.enqueue(JobSpec {
            key: "k".into(),
            label: String::new(),
            resources: vec![],
            kind: JobKind::Transfer,
            queue: None,
            work: Box::new(|_| Ok(()))
        }),
        id
    );
    manager.set_paused(&id, true);
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(manager.jobs()[0].state, JobState::Paused);
    manager.set_paused(&id, false);
    let deadline = Instant::now() + Duration::from_secs(5);
    while manager.is_busy() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(manager.jobs()[0].state, JobState::Done);
    std::thread::sleep(Duration::from_millis(400));
    assert_eq!(seen.lock()[0].state, JobState::Done);
}

#[test]
fn manager_runs_jobs_sharing_a_device_concurrently() {
    let manager = TransferManager::new(|_| {});
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let mut releases = Vec::new();
    for n in 0..2 {
        let started = started_tx.clone();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        releases.push(release_tx);
        manager.enqueue(JobSpec {
            key: format!("job{n}"),
            label: String::new(),
            resources: vec![ResourceClaim::shared("device:shared")],
            kind: JobKind::Transfer,
            queue: None,
            work: Box::new(move |_| {
                started.send(n).map_err(|e| e.to_string())?;
                release_rx.recv().map_err(|e| e.to_string())?;
                Ok(())
            }),
        });
    }
    drop(started_tx);
    let mut started = vec![
        started_rx.recv_timeout(Duration::from_secs(5)).unwrap(),
        started_rx.recv_timeout(Duration::from_secs(5)).unwrap(),
    ];
    started.sort_unstable();
    assert_eq!(started, vec![0, 1]);
    for release in releases {
        release.send(()).unwrap();
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    while manager.is_busy() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(!manager.is_busy());
}

#[test]
fn manager_caps_parallel_jobs_at_three_and_reuses_finished_slots() {
    use std::sync::mpsc;

    let manager = TransferManager::new(|_| {});
    let (started_tx, started_rx) = mpsc::channel();
    let mut releases = Vec::new();
    for (n, kind) in [
        JobKind::Transfer,
        JobKind::Check,
        JobKind::Transfer,
        JobKind::Check,
        JobKind::Transfer,
    ]
    .into_iter()
    .enumerate()
    {
        let (release_tx, release_rx) = mpsc::channel();
        releases.push(release_tx);
        let started = started_tx.clone();
        manager.enqueue(JobSpec {
            key: format!("job{n}"),
            label: String::new(),
            resources: vec![
                ResourceClaim::shared(format!("device:source{n}")),
                ResourceClaim::shared("device:destination"),
                ResourceClaim::exclusive(format!("destination-path:{n}")),
            ],
            kind,
            queue: None,
            work: Box::new(move |_| {
                started.send(n).map_err(|e| e.to_string())?;
                release_rx.recv().map_err(|e| e.to_string())?;
                Ok(())
            }),
        });
    }
    drop(started_tx);

    let mut initial = Vec::new();
    for _ in 0..3 {
        initial.push(started_rx.recv_timeout(Duration::from_secs(5)).unwrap());
    }
    initial.sort_unstable();
    assert_eq!(initial, vec![0, 1, 2]);
    assert!(matches!(
        started_rx.recv_timeout(Duration::from_millis(100)),
        Err(mpsc::RecvTimeoutError::Timeout)
    ));

    releases[0].send(()).unwrap();
    assert_eq!(started_rx.recv_timeout(Duration::from_secs(5)).unwrap(), 3);
    assert!(matches!(
        started_rx.recv_timeout(Duration::from_millis(100)),
        Err(mpsc::RecvTimeoutError::Timeout)
    ));
    releases[1].send(()).unwrap();
    assert_eq!(started_rx.recv_timeout(Duration::from_secs(5)).unwrap(), 4);

    for release in releases.iter().skip(2) {
        release.send(()).unwrap();
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    while manager.is_busy() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(!manager.is_busy());
    assert!(manager.jobs().iter().all(|job| job.state == JobState::Done));
}

#[test]
fn manager_serialises_same_destination_path_and_excludes_wipes() {
    use std::sync::mpsc;

    let manager = TransferManager::new(|_| {});
    let (started_tx, started_rx) = mpsc::channel();
    let mut releases = Vec::new();
    for (n, resources) in [
        vec![
            ResourceClaim::shared("device:source-a"),
            ResourceClaim::shared("device:nas"),
            ResourceClaim::exclusive("destination-path:nas:photos"),
        ],
        vec![
            ResourceClaim::shared("device:source-b"),
            ResourceClaim::shared("device:nas"),
            ResourceClaim::exclusive("destination-path:nas:photos"),
        ],
        vec![ResourceClaim::exclusive("device:nas")],
        vec![
            ResourceClaim::shared("device:source-c"),
            ResourceClaim::shared("device:nas"),
            ResourceClaim::exclusive("destination-path:nas:videos"),
        ],
    ]
    .into_iter()
    .enumerate()
    {
        let started = started_tx.clone();
        let (release_tx, release_rx) = mpsc::channel();
        releases.push(release_tx);
        manager.enqueue(JobSpec {
            key: format!("job{n}"),
            label: String::new(),
            resources,
            kind: if n == 2 {
                JobKind::Wipe
            } else {
                JobKind::Transfer
            },
            queue: None,
            work: Box::new(move |_| {
                started.send(n).map_err(|e| e.to_string())?;
                release_rx.recv().map_err(|e| e.to_string())?;
                Ok(())
            }),
        });
    }
    drop(started_tx);

    let mut initially_started = vec![
        started_rx.recv_timeout(Duration::from_secs(5)).unwrap(),
        started_rx.recv_timeout(Duration::from_secs(5)).unwrap(),
    ];
    initially_started.sort_unstable();
    assert_eq!(initially_started, vec![0, 3]);
    assert!(matches!(
        started_rx.recv_timeout(Duration::from_millis(100)),
        Err(mpsc::RecvTimeoutError::Timeout)
    ));

    releases[0].send(()).unwrap();
    assert_eq!(started_rx.recv_timeout(Duration::from_secs(5)).unwrap(), 1);
    releases[1].send(()).unwrap();
    assert!(matches!(
        started_rx.recv_timeout(Duration::from_millis(100)),
        Err(mpsc::RecvTimeoutError::Timeout)
    ));
    releases[3].send(()).unwrap();
    assert_eq!(started_rx.recv_timeout(Duration::from_secs(5)).unwrap(), 2);
    releases[2].send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while manager.is_busy() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(!manager.is_busy());
}
