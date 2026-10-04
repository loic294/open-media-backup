use super::*;
use crate::domain::FileCopy;
use crate::plan::{classify_flow, Catalog, Category, FailureMap, WorkspaceContext};
use crate::testing::{write, Fixture};
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn dest(fx: &Fixture) -> std::path::PathBuf {
    fx.nas_dir.path().join("photo/Trip/Camera A Card 1")
}

fn wait_for(what: &str, mut cond: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !cond() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn transfer_job(fx: &Fixture, manager: &TransferManager, queue: &Arc<ConflictQueue>) -> String {
    let (store, nas, card) = (fx.store.clone(), fx.nas.id.clone(), fx.card.id.clone());
    let roots = fx.resolver.0.lock().clone();
    manager.enqueue(JobSpec {
        key: format!("flow-{}", Arc::strong_count(queue)),
        label: String::new(),
        resources: vec![
            ResourceClaim::shared(format!("device:{card}")),
            ResourceClaim::shared(format!("device:{nas}")),
            ResourceClaim::exclusive(format!("destination-path:{nas}:photo/Trip/Camera A Card 1")),
        ],
        kind: JobKind::Transfer,
        queue: Some(queue.clone()),
        work: Box::new(move |h| {
            let resolver = crate::testing::MapResolver(Mutex::new(roots));
            run_transfer(
                &store,
                &resolver,
                "project",
                "flow",
                h,
                &Mutex::new(FailureMap::new()),
            )
        }),
    })
}

fn job(manager: &TransferManager, id: &str) -> TransferJob {
    manager.jobs().into_iter().find(|j| j.id == id).unwrap()
}

fn conflicted() -> Fixture {
    let fx = Fixture::new();
    fx.write_card_file("DCIM/A.JPG", b"card a");
    fx.write_card_file("DCIM/B.JPG", b"card b");
    write(&dest(&fx), "A.JPG", b"old a");
    write(&dest(&fx), "B.JPG", b"old b");
    fx
}

#[test]
fn every_conflict_prompts_and_stale_replies_are_rejected() {
    let fx = conflicted();
    let manager = TransferManager::new(|_| {});
    let id = transfer_job(&fx, &manager, &ConflictQueue::new());
    wait_for("first request", || {
        job(&manager, &id).state == JobState::AwaitingDecision
    });
    let first = job(&manager, &id).pending_conflict.unwrap();
    assert!(first.source_path.ends_with("A.JPG"));
    assert_ne!(first.source_hash, first.destination_hash);
    assert!(manager
        .resolve_conflict(&id, "stale", ConflictDecision::Replace, true)
        .is_err());
    assert!(manager
        .resolve_conflict("nope", &first.request_id, ConflictDecision::Replace, false)
        .is_err());
    manager
        .resolve_conflict(&id, &first.request_id, ConflictDecision::Skip, false)
        .unwrap();
    assert!(manager
        .resolve_conflict(&id, &first.request_id, ConflictDecision::Replace, false)
        .is_err());
    wait_for("second request", || {
        job(&manager, &id)
            .pending_conflict
            .is_some_and(|p| p.request_id != first.request_id)
    });
    let second = job(&manager, &id).pending_conflict.unwrap();
    manager
        .resolve_conflict(&id, &second.request_id, ConflictDecision::KeepBoth, false)
        .unwrap();
    wait_for("done", || job(&manager, &id).state == JobState::Done);
    let d = dest(&fx);
    assert_eq!(std::fs::read(d.join("A.JPG")).unwrap(), b"old a");
    assert_eq!(std::fs::read(d.join("B.JPG")).unwrap(), b"old b");
    assert_eq!(std::fs::read(d.join("B (1).JPG")).unwrap(), b"card b");
    assert!(!d.join("A (1).JPG").exists());
    // The skipped file stays pending and nothing is claimed for it.
    let copies = fx.store.list::<FileCopy>().unwrap();
    assert!(copies
        .iter()
        .all(|c| c.path != "A.JPG" && !c.path.ends_with("/A.JPG")));
    assert!(job(&manager, &id).pending_conflict.is_none());
}

#[test]
fn apply_to_remaining_covers_waiting_and_later_conflicts_in_one_queue_only() {
    let fx = conflicted();
    let manager = TransferManager::new(|_| {});
    let id = transfer_job(&fx, &manager, &ConflictQueue::new());
    wait_for("request", || job(&manager, &id).pending_conflict.is_some());
    let request = job(&manager, &id).pending_conflict.unwrap();
    manager
        .resolve_conflict(&id, &request.request_id, ConflictDecision::Replace, true)
        .unwrap();
    wait_for("done", || job(&manager, &id).state == JobState::Done);
    assert_eq!(std::fs::read(dest(&fx).join("A.JPG")).unwrap(), b"card a");
    assert_eq!(std::fs::read(dest(&fx).join("B.JPG")).unwrap(), b"card b");
    let copies = fx.store.list::<FileCopy>().unwrap();
    assert!(copies.iter().filter(|c| !c.removed).count() >= 4);

    // A new queue does not inherit the preference.
    let fx2 = conflicted();
    let manager2 = TransferManager::new(|_| {});
    let id2 = transfer_job(&fx2, &manager2, &ConflictQueue::new());
    wait_for("fresh request", || {
        job(&manager2, &id2).pending_conflict.is_some()
    });
    manager2.cancel(&id2);
    wait_for("cancelled", || {
        job(&manager2, &id2).state == JobState::Cancelled
    });
    assert_eq!(std::fs::read(dest(&fx2).join("A.JPG")).unwrap(), b"old a");
}

#[test]
fn apply_to_remaining_answers_other_waiting_jobs_of_the_queue() {
    let queue = ConflictQueue::new();
    let manager = TransferManager::new(|_| {});
    let mut ids = Vec::new();
    for n in 0..2 {
        let q = queue.clone();
        ids.push(manager.enqueue(JobSpec {
            key: format!("k{n}"),
            label: String::new(),
            resources: vec![],
            kind: JobKind::Transfer,
            queue: Some(q),
            work: Box::new(|h| {
                let info = ConflictInfo {
                    source_path: "s".into(),
                    destination_path: "d".into(),
                    source_hash: "1".into(),
                    destination_hash: "2".into(),
                };
                let decision = h
                    .request_decision(&info)
                    .map_err(|_| "cancelled".to_string())?;
                if decision == ConflictDecision::Skip {
                    Ok(())
                } else {
                    Err("unexpected".into())
                }
            }),
        }));
    }
    wait_for("both waiting", || {
        ids.iter()
            .all(|i| job(&manager, i).pending_conflict.is_some())
    });
    let other = Arc::new(ConflictQueue::default());
    let unrelated = {
        let o = other.clone();
        manager.enqueue(JobSpec {
            key: "unrelated".into(),
            label: String::new(),
            resources: vec![],
            kind: JobKind::Transfer,
            queue: Some(o),
            work: Box::new(|h| {
                h.request_decision(&ConflictInfo {
                    source_path: "s".into(),
                    destination_path: "d".into(),
                    source_hash: "1".into(),
                    destination_hash: "2".into(),
                })
                .map(|_| ())
                .map_err(|_| "cancelled".to_string())
            }),
        })
    };
    wait_for("unrelated waiting", || {
        job(&manager, &unrelated).pending_conflict.is_some()
    });
    let request = job(&manager, &ids[0]).pending_conflict.unwrap();
    manager
        .resolve_conflict(&ids[0], &request.request_id, ConflictDecision::Skip, true)
        .unwrap();
    wait_for("queue done", || {
        ids.iter().all(|i| job(&manager, i).state == JobState::Done)
    });
    assert_eq!(job(&manager, &unrelated).state, JobState::AwaitingDecision);
    manager.cancel(&unrelated);
    wait_for("unrelated cancelled", || {
        job(&manager, &unrelated).state == JobState::Cancelled
    });
}

#[test]
fn cancelling_a_waiting_job_unblocks_the_device() {
    let fx = conflicted();
    let manager = TransferManager::new(|_| {});
    let first = transfer_job(&fx, &manager, &ConflictQueue::new());
    wait_for("waiting", || {
        job(&manager, &first).pending_conflict.is_some()
    });
    let queued = manager.enqueue(JobSpec {
        key: "later".into(),
        label: String::new(),
        resources: vec![
            ResourceClaim::shared(format!("device:{}", fx.nas.id)),
            ResourceClaim::exclusive(format!(
                "destination-path:{}:photo/Trip/Camera A Card 1",
                fx.nas.id
            )),
        ],
        kind: JobKind::Transfer,
        queue: None,
        work: Box::new(|_| Ok(())),
    });
    assert_eq!(job(&manager, &queued).state, JobState::Queued);
    manager.cancel(&first);
    wait_for("later job", || {
        job(&manager, &queued).state == JobState::Done
    });
    assert_eq!(job(&manager, &first).state, JobState::Cancelled);
    assert_eq!(std::fs::read(dest(&fx).join("A.JPG")).unwrap(), b"old a");
}

fn check_handle() -> JobHandle {
    JobHandle::new(
        TransferJob::new("c".into(), "flow".into(), String::new(), JobKind::Check),
        Arc::new(|| {}),
    )
}

fn observed_check_handle(
    observe: impl Fn(&JobHandle, &TransferJob) + Send + Sync + 'static,
) -> (Arc<JobHandle>, Arc<Mutex<Vec<TransferJob>>>) {
    let snapshots = Arc::new(Mutex::new(Vec::new()));
    let captured = snapshots.clone();
    let handle = Arc::new_cyclic(|weak: &std::sync::Weak<JobHandle>| {
        let weak = weak.clone();
        JobHandle::new(
            TransferJob::new("c".into(), "flow".into(), String::new(), JobKind::Check),
            Arc::new(move || {
                let handle = weak.upgrade().unwrap();
                let job = handle.snapshot();
                captured.lock().push(job.clone());
                observe(&handle, &job);
            }),
        )
    });
    (handle, snapshots)
}

fn run_check(fx: &Fixture, handle: &JobHandle) {
    let context = WorkspaceContext::for_project(&fx.store, "project").unwrap();
    run_workspace_check(&fx.store, &fx.resolver, &context, "flow", handle).unwrap();
}

fn assert_check_progress(snapshots: &[TransferJob]) {
    assert!(snapshots
        .windows(2)
        .all(|pair| pair[0].bytes_done <= pair[1].bytes_done));
    assert!(snapshots
        .iter()
        .all(|job| job.bytes_done <= job.bytes_total));
    let last = snapshots.last().unwrap();
    assert_eq!(last.bytes_done, last.bytes_total);
    assert_eq!(last.files_done, last.files_total);
}

#[test]
fn check_reports_chunk_progress_during_both_reads_without_overcounting() {
    let size = 3 * crate::hashing::BUFFER_SIZE;
    for destination_size in [size / 2, size, size * 2] {
        let fx = Fixture::new();
        fx.write_card_file("DCIM/A.JPG", &vec![1; size]);
        write(&dest(&fx), "A.JPG", &vec![1; destination_size]);
        let (handle, snapshots) = observed_check_handle(|_, _| {});
        run_check(&fx, &handle);
        let snapshots = snapshots.lock();
        assert_check_progress(&snapshots);
        assert!(snapshots.iter().any(|job| job.files_done == 0
            && job.bytes_done > 0
            && job.bytes_done < (size / 2) as u64
            && job.current_file.as_deref() == Some("A.JPG")));
        assert!(snapshots.iter().any(|job| job.files_done == 0
            && job.bytes_done > (size / 2) as u64
            && job.bytes_done < size as u64));
        let results = handle.snapshot().check_results.unwrap();
        assert_eq!(results.matched, usize::from(destination_size == size));
        assert_eq!(results.conflicts, usize::from(destination_size != size));
    }
}

#[test]
fn check_finishes_missing_empty_and_unreadable_files_at_exact_totals() {
    for outcome in ["missing", "empty", "unreadable"] {
        let fx = Fixture::new();
        fx.write_card_file("DCIM/A.JPG", if outcome == "empty" { b"" } else { b"data" });
        match outcome {
            "empty" => {
                write(&dest(&fx), "A.JPG", b"");
            }
            "unreadable" => std::fs::create_dir_all(dest(&fx).join("A.JPG")).unwrap(),
            _ => {}
        }
        let (handle, snapshots) = observed_check_handle(|_, _| {});
        run_check(&fx, &handle);
        assert_check_progress(&snapshots.lock());
        let results = handle.snapshot().check_results.unwrap();
        assert_eq!(results.missing, usize::from(outcome == "missing"));
        assert_eq!(results.matched, usize::from(outcome == "empty"));
        assert_eq!(results.errors, usize::from(outcome == "unreadable"));
        assert_eq!(handle.snapshot().files_done, 1);
    }
}

#[test]
fn check_destination_read_failure_after_source_progress_finishes_at_exact_totals() {
    let fx = Fixture::new();
    let size = 3 * crate::hashing::BUFFER_SIZE;
    fx.write_card_file("DCIM/A.JPG", &vec![1; size]);
    let target = write(&dest(&fx), "A.JPG", &vec![1; size]);
    let changed = std::sync::atomic::AtomicBool::new(false);
    let (handle, snapshots) = observed_check_handle(move |_, job| {
        if job.bytes_done > 0 && !changed.swap(true, std::sync::atomic::Ordering::SeqCst) {
            std::fs::remove_file(&target).unwrap();
            std::fs::create_dir(&target).unwrap();
        }
    });
    run_check(&fx, &handle);
    assert_check_progress(&snapshots.lock());
    let results = handle.snapshot().check_results.unwrap();
    assert_eq!(results.errors, 1);
    assert!(results.items[0].error.is_some());
    assert!(fx.store.list::<FileCopy>().unwrap().is_empty());
}

#[test]
fn check_missing_destination_reports_source_claim_revalidation_progress() {
    let fx = Fixture::new();
    fx.write_card_file("DCIM/A.JPG", &vec![1; 3 * crate::hashing::BUFFER_SIZE]);
    run_transfer(
        &fx.store,
        &fx.resolver,
        "project",
        "flow",
        &check_handle(),
        &Mutex::new(FailureMap::new()),
    )
    .unwrap();
    std::fs::remove_file(dest(&fx).join("A.JPG")).unwrap();
    let (handle, snapshots) = observed_check_handle(|_, _| {});
    run_check(&fx, &handle);
    let snapshots = snapshots.lock();
    assert_check_progress(&snapshots);
    assert!(snapshots
        .iter()
        .any(|job| job.files_done == 0 && job.bytes_done > 0 && job.bytes_done < job.bytes_total));
    assert_eq!(handle.snapshot().check_results.unwrap().missing, 1);
}

#[test]
fn check_cancellation_during_either_read_keeps_partial_progress_without_verifying() {
    let size = 3 * crate::hashing::BUFFER_SIZE;
    for cancel_after in [0, size as u64 / 2] {
        let fx = Fixture::new();
        fx.write_card_file("DCIM/A.JPG", &vec![1; size]);
        write(&dest(&fx), "A.JPG", &vec![1; size]);
        let (handle, snapshots) = observed_check_handle(move |handle, job| {
            if job.bytes_done > cancel_after {
                handle.cancel();
            }
        });
        run_check(&fx, &handle);
        let job = handle.snapshot();
        assert!(handle.is_cancelled());
        assert!(job.bytes_done > cancel_after && job.bytes_done < job.bytes_total);
        assert_eq!(job.files_done, 0);
        assert_eq!(job.check_results.unwrap(), CheckResults::default());
        assert!(fx.store.list::<FileCopy>().unwrap().is_empty());
        assert!(snapshots
            .lock()
            .windows(2)
            .all(|pair| pair[0].bytes_done <= pair[1].bytes_done));
    }
}

#[test]
fn check_pause_blocks_chunk_progress_until_resumed() {
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    };
    let fx = Fixture::new();
    let size = 3 * crate::hashing::BUFFER_SIZE;
    fx.write_card_file("DCIM/A.JPG", &vec![1; size]);
    write(&dest(&fx), "A.JPG", &vec![1; size]);
    let paused_once = AtomicBool::new(false);
    let (paused_tx, paused_rx) = mpsc::channel();
    let (handle, snapshots) = observed_check_handle(move |handle, job| {
        if job.bytes_done > 0 && !paused_once.swap(true, Ordering::SeqCst) {
            handle.set_paused(true);
            paused_tx.send(()).unwrap();
        }
    });
    let worker_handle = handle.clone();
    let (done_tx, done_rx) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        run_check(&fx, &worker_handle);
        done_tx.send(()).unwrap();
    });
    paused_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    let paused_bytes = handle.snapshot().bytes_done;
    let blocked = done_rx.recv_timeout(Duration::from_millis(200));
    let after = handle.snapshot();
    handle.set_paused(false);
    worker.join().unwrap();
    assert_eq!(blocked, Err(mpsc::RecvTimeoutError::Timeout));
    assert_eq!(after.state, JobState::Paused);
    assert_eq!(after.bytes_done, paused_bytes);
    assert_check_progress(&snapshots.lock());
    assert_eq!(handle.snapshot().check_results.unwrap().matched, 1);
}

fn check(fx: &Fixture) -> (JobHandle, Result<(), String>) {
    let h = check_handle();
    let context = WorkspaceContext::for_project(&fx.store, "project").unwrap();
    let result = run_workspace_check(&fx.store, &fx.resolver, &context, "flow", &h);
    (h, result)
}

fn tree(root: &std::path::Path) -> Vec<(String, Vec<u8>)> {
    fn walk(dir: &std::path::Path, root: &std::path::Path, out: &mut Vec<(String, Vec<u8>)>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, root, out);
            } else {
                out.push((
                    path.strip_prefix(root).unwrap().display().to_string(),
                    std::fs::read(&path).unwrap(),
                ));
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort();
    out
}

#[test]
fn check_reports_outcomes_without_touching_media_or_markers() {
    let mut fx = Fixture::new();
    fx.destination.use_backup_marker = true;
    fx.space.backup_marker_template = "{source_name}-backup".into();
    fx.store.put(&fx.destination).unwrap();
    fx.store.put(&fx.space).unwrap();
    fx.write_card_file("DCIM/A.JPG", b"same");
    fx.write_card_file("DCIM/B.JPG", b"card b");
    fx.write_card_file("DCIM/C.JPG", b"card c");
    // Resolve the marker-derived folder without creating the marker.
    let ctx = crate::plan::resolve_flow(&fx.store, &fx.resolver, "project", "flow").unwrap();
    let base = fx.nas_dir.path().join(ctx.target_rel(""));
    write(&base, "A.JPG", b"same");
    write(&base, "B.JPG", b"different");
    write(&base, "Z.JPG", b"unrelated extra");
    let (card_before, nas_before) = (tree(fx.card_dir.path()), tree(fx.nas_dir.path()));
    let (h, result) = check(&fx);
    result.unwrap();
    let job = h.snapshot();
    let results = job.check_results.unwrap();
    assert_eq!(
        (
            results.matched,
            results.missing,
            results.conflicts,
            results.errors
        ),
        (1, 1, 1, 0)
    );
    assert_eq!(job.kind, JobKind::Check);
    assert!(results.items.iter().any(|i| i.source_path == "B.JPG"
        && i.outcome == CheckOutcome::Conflict
        && i.destination_path.ends_with("B.JPG")));
    assert!(results
        .items
        .iter()
        .any(|i| i.source_path == "C.JPG" && i.outcome == CheckOutcome::Missing));
    assert_eq!(tree(fx.card_dir.path()), card_before);
    assert_eq!(tree(fx.nas_dir.path()), nas_before);
    assert!(crate::paths::read_backup_folder(fx.card_dir.path()).is_none());
    assert!(fx
        .store
        .list::<FileCopy>()
        .unwrap()
        .iter()
        .all(|c| c.path.ends_with("A.JPG")));
    assert_eq!(fx.store.list::<FileCopy>().unwrap().len(), 2);
    // Check progress never feeds learned transfer speeds.
    assert!(crate::app::AppSettings::load(&fx.store)
        .transfer_speeds
        .is_empty());
}

#[test]
fn check_rechecks_transferred_files_and_invalidates_stale_claims() {
    let fx = Fixture::new();
    fx.write_card_file("DCIM/A.JPG", b"aaaa");
    fx.write_card_file("DCIM/B.JPG", b"bbbb");
    let h = check_handle();
    run_transfer(
        &fx.store,
        &fx.resolver,
        "project",
        "flow",
        &h,
        &Mutex::new(FailureMap::new()),
    )
    .unwrap();
    let d = dest(&fx);
    // Matching copies stay verified; a deleted copy and a same-size edit are detected.
    std::fs::remove_file(d.join("A.JPG")).unwrap();
    std::fs::write(d.join("B.JPG"), b"BBBB").unwrap();
    let (h, result) = check(&fx);
    result.unwrap();
    let results = h.snapshot().check_results.unwrap();
    assert_eq!(
        (results.missing, results.conflicts, results.matched),
        (1, 1, 0)
    );
    let catalog = Catalog::load(&fx.store).unwrap();
    assert!(catalog
        .copy_at("nas", "photo/Trip/Camera A Card 1/A.JPG")
        .is_none());
    assert!(catalog
        .copy_at("nas", "photo/Trip/Camera A Card 1/B.JPG")
        .is_none());
    // Source claims of unchanged sources remain.
    assert!(catalog.copy_at("card", "DCIM/A.JPG").is_some());
    let ctx = crate::plan::resolve_flow(&fx.store, &fx.resolver, "project", "flow").unwrap();
    let planned = classify_flow(&ctx, &catalog, None);
    assert!(planned.iter().all(|f| f.category == Category::ToTransfer));
    // Restoring the file makes the next check record it as verified again.
    std::fs::write(d.join("A.JPG"), b"aaaa").unwrap();
    let (h, _) = check(&fx);
    assert_eq!(h.snapshot().check_results.unwrap().matched, 1);
    let catalog = Catalog::load(&fx.store).unwrap();
    assert!(catalog
        .copy_at("nas", "photo/Trip/Camera A Card 1/A.JPG")
        .is_some());
}

#[test]
fn check_cancelled_before_work_persists_nothing() {
    let fx = Fixture::new();
    fx.write_card_file("DCIM/A.JPG", b"aaaa");
    write(&dest(&fx), "A.JPG", b"aaaa");
    let h = check_handle();
    h.cancel();
    let context = WorkspaceContext::for_project(&fx.store, "project").unwrap();
    run_workspace_check(&fx.store, &fx.resolver, &context, "flow", &h).unwrap();
    assert!(fx.store.list::<FileCopy>().unwrap().is_empty());
    assert_eq!(h.snapshot().check_results.unwrap().matched, 0);
}

#[test]
fn check_requires_connected_devices() {
    let fx = Fixture::new();
    fx.unmount("nas");
    assert!(check(&fx).1.unwrap_err().contains("Home NAS"));
}

fn same_size_source_edit_fixture() -> (Fixture, crate::domain::Device) {
    let fx = Fixture::new();
    let backup = crate::domain::Device {
        id: "backup".into(),
        name: "Backup".into(),
        role: crate::domain::DeviceRole::Final,
        ..Default::default()
    };
    fx.store.put(&backup).unwrap();
    fx.write_card_file("DCIM/A.JPG", b"aaaa");
    run_transfer(
        &fx.store,
        &fx.resolver,
        "project",
        "flow",
        &check_handle(),
        &Mutex::new(FailureMap::new()),
    )
    .unwrap();
    // The old content also lives on an unrelated destination.
    let old_id = Catalog::load(&fx.store)
        .unwrap()
        .file_at("card", "DCIM/A.JPG", Some(4))
        .unwrap()
        .id
        .clone();
    fx.store
        .put(&FileCopy {
            id: FileCopy::id_for(&old_id, "backup", "old/A.JPG"),
            file_id: old_id,
            device_id: "backup".into(),
            path: "old/A.JPG".into(),
            verified_at: 1,
            removed: false,
        })
        .unwrap();
    fx.write_card_file("DCIM/A.JPG", b"AAAA");
    (fx, backup)
}

#[test]
fn check_invalidates_stale_source_identity_after_same_size_edit() {
    for dest_state in ["missing", "mismatched"] {
        let (fx, _) = same_size_source_edit_fixture();
        let target = dest(&fx).join("A.JPG");
        if dest_state == "missing" {
            std::fs::remove_file(&target).unwrap();
        } else {
            std::fs::write(&target, b"zzzz").unwrap();
        }
        let (h, result) = check(&fx);
        result.unwrap();
        assert_eq!(h.snapshot().check_results.unwrap().matched, 0);
        let catalog = Catalog::load(&fx.store).unwrap();
        assert!(
            catalog.copy_at("card", "DCIM/A.JPG").is_none(),
            "{dest_state}"
        );
        // Unrelated copies of the old content are untouched.
        assert!(catalog.copy_at("backup", "old/A.JPG").is_some());
        let ctx = crate::plan::resolve_flow(&fx.store, &fx.resolver, "project", "flow").unwrap();
        assert!(classify_flow(&ctx, &catalog, None)
            .iter()
            .all(|f| f.category == Category::ToTransfer));
    }
}

#[test]
fn transfer_after_same_size_source_edit_recatalogs_fresh_identity() {
    let (fx, _) = same_size_source_edit_fixture();
    let target = dest(&fx).join("A.JPG");
    std::fs::remove_file(&target).unwrap();
    // Stale catalog claims still make the file look transferred; a check exposes it.
    check(&fx).1.unwrap();
    run_transfer(
        &fx.store,
        &fx.resolver,
        "project",
        "flow",
        &check_handle(),
        &Mutex::new(FailureMap::new()),
    )
    .unwrap();
    assert_eq!(std::fs::read(&target).unwrap(), b"AAAA");
    let catalog = Catalog::load(&fx.store).unwrap();
    let record = catalog.file_at("card", "DCIM/A.JPG", Some(4)).unwrap();
    assert_eq!(
        record.hash,
        crate::hashing::hash_file(&target, record.hash_algo, |_| true).unwrap()
    );
    assert!(catalog.copy_at("backup", "old/A.JPG").is_some());
}

#[test]
fn skipped_conflicts_do_not_teach_transfer_speeds() {
    let fx = Fixture::new();
    fx.write_card_file("DCIM/A.JPG", &vec![1u8; 60 * 1024 * 1024]);
    write(&dest(&fx), "A.JPG", b"old");
    let manager = TransferManager::new(|_| {});
    let id = transfer_job(&fx, &manager, &ConflictQueue::new());
    wait_for("request", || job(&manager, &id).pending_conflict.is_some());
    std::thread::sleep(Duration::from_millis(2100));
    let request = job(&manager, &id).pending_conflict.unwrap();
    manager
        .resolve_conflict(&id, &request.request_id, ConflictDecision::Skip, false)
        .unwrap();
    wait_for("done", || job(&manager, &id).state == JobState::Done);
    assert!(crate::app::AppSettings::load(&fx.store)
        .transfer_speeds
        .is_empty());
}
