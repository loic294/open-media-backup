use super::copy::hash_checked_with_progress;
use super::history::HistoryRecorder;
use super::metrics::tests::{context, TestClock};
use super::metrics::{AnalysisKind, AnalysisState};
use super::*;
use crate::domain::{HashAlgo, VerifyMode};
use crate::plan::{resolve_workspace_flow, WorkspaceContext};
use crate::store::analysis::{AnalysisFilter, AnalysisJobsRequest};
use crate::store::Store;
use crate::testing::Fixture;
use parking_lot::Mutex;
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

fn handle(kind: JobKind) -> JobHandle {
    JobHandle::new(
        TransferJob::new("j".into(), "flow".into(), "Test".into(), kind),
        Arc::new(|| {}),
    )
    .with_analysis(Some(context()), None)
}

fn metrics(h: &JobHandle) -> AnalysisMetrics {
    h.snapshot().analysis.unwrap().metrics
}

fn wait(mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !done() {
        assert!(Instant::now() < deadline, "analysis operation timed out");
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn page(store: &Store) -> crate::store::analysis::AnalysisJobPage {
    store
        .list_speed_analysis_jobs(&AnalysisJobsRequest {
            filter: Default::default(),
            offset: 0,
            limit: 100,
        })
        .unwrap()
}

#[test]
fn analysis_inline_hashing_is_not_a_separate_read_and_progress_is_unchanged() {
    for verify in [VerifyMode::Inline, VerifyMode::Reread] {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("source");
        let dst = dir.path().join("destination");
        std::fs::write(&src, b"abcdef").unwrap();
        let h = handle(JobKind::Transfer);
        h.start_work();
        copy_verified(&src, &dst, HashAlgo::Blake3, verify, None, &h).unwrap();
        h.update(|j| j.state = JobState::Done);
        let m = metrics(&h);
        assert_eq!(m.copy_bytes, 6);
        assert_eq!(m.committed_bytes, 6);
        assert_eq!(m.transferred_files, 1);
        assert_eq!(m.source_check_bytes, 0);
        assert_eq!(m.source_check_secs, 0.0);
        assert_eq!(
            m.destination_check_bytes,
            if verify == VerifyMode::Reread { 6 } else { 0 }
        );
        assert_eq!(h.snapshot().bytes_done, 6);
        assert_eq!(m.adopted_files + m.skipped_files, 0);
    }
}

#[test]
fn analysis_adoption_skip_empty_and_retry_io_are_not_committed_budgets() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("source");
    let dst = dir.path().join("destination");
    std::fs::write(&src, b"abcdef").unwrap();
    std::fs::write(&dst, b"abcdef").unwrap();
    let h = handle(JobKind::Transfer);
    h.start_work();
    let adopted =
        copy_verified(&src, &dst, HashAlgo::Blake3, VerifyMode::Reread, None, &h).unwrap();
    assert!(adopted.adopted);
    std::fs::write(&dst, b"other").unwrap();
    let skipped = copy_resolving(
        &src,
        &dst,
        HashAlgo::Blake3,
        VerifyMode::Reread,
        None,
        &h,
        &mut |_| Ok(ConflictDecision::Skip),
    )
    .unwrap();
    assert!(skipped.skipped);
    let m = metrics(&h);
    assert_eq!(m.adopted_files, 1);
    assert_eq!(m.skipped_files, 1);
    assert_eq!(m.copy_bytes, 0);
    assert_eq!(m.committed_bytes, 0);
    assert_eq!(m.source_check_bytes, 18);
    assert_eq!(m.destination_check_bytes, 16);
    assert_eq!(h.snapshot().bytes_done, 0);

    let dst = dir.path().join("new");
    assert!(matches!(
        copy_verified(
            &src,
            &dst,
            HashAlgo::Blake3,
            VerifyMode::Reread,
            Some("stale"),
            &h
        ),
        Err(CopyError::SourceChanged)
    ));
    copy_verified(&src, &dst, HashAlgo::Blake3, VerifyMode::Reread, None, &h).unwrap();
    let m = metrics(&h);
    assert_eq!(m.copy_bytes, 12);
    assert_eq!(m.committed_bytes, 6);
    assert_eq!(m.transferred_files, 1);
    std::fs::write(&src, b"").unwrap();
    copy_verified(
        &src,
        &dir.path().join("empty"),
        HashAlgo::Blake3,
        VerifyMode::Reread,
        None,
        &h,
    )
    .unwrap();
    h.update(|j| j.bytes_done = 999_999);
    assert_eq!(metrics(&h).committed_bytes, 6);
    assert_eq!(metrics(&h).transferred_files, 2);
}

#[test]
fn analysis_replacement_counts_target_and_staged_destination_rereads() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("source");
    let dst = dir.path().join("destination");
    std::fs::write(&src, b"new").unwrap();
    std::fs::write(&dst, b"old!!").unwrap();
    let h = handle(JobKind::Transfer);
    h.start_work();
    let outcome = copy_resolving(
        &src,
        &dst,
        HashAlgo::Blake3,
        VerifyMode::Reread,
        None,
        &h,
        &mut |_| Ok(ConflictDecision::Replace),
    )
    .unwrap();
    assert!(outcome.replaced);
    let m = metrics(&h);
    assert_eq!(m.source_check_bytes, 3);
    assert_eq!(m.destination_check_bytes, 5 + 3 + 5);
    assert_eq!(m.copy_bytes, 3);
    assert_eq!(m.committed_bytes, 3);
    assert_eq!(m.transferred_files, 1);
}

#[test]
fn analysis_hash_read_is_counted_before_cancellation_and_pause_excludes_sleep() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("file");
    std::fs::write(&path, b"abcdef").unwrap();
    let clock = Arc::new(TestClock::default());
    let h = handle(JobKind::Check).with_analysis_clock(clock.clone());
    h.start_work();
    hash_checked_with_progress(&path, HashAlgo::Blake3, &h, true, |_, _| {
        clock.advance(2);
        h.set_paused(true);
        clock.advance(10);
        h.set_paused(false);
        assert_eq!(
            h.snapshot().analysis.unwrap().phase,
            AnalysisPhase::SourceCheck
        );
        clock.advance(3);
    })
    .unwrap();
    let m = metrics(&h);
    assert_eq!(m.source_check_bytes, 6);
    assert_eq!(m.source_check_secs, 5.0);
    assert_eq!(m.paused_secs, 10.0);
    assert_eq!(m.copy_bytes, 0);
    let result = hash_checked_with_progress(&path, HashAlgo::Blake3, &h, false, |_, _| h.cancel());
    assert!(matches!(result, Err(CopyError::Cancelled)));
    assert_eq!(metrics(&h).destination_check_bytes, 6);
    h.update(|j| j.state = JobState::Cancelled);
    let frozen = h.snapshot().analysis.unwrap();
    clock.advance(100);
    h.analysis_metrics(|m| m.copy_bytes += 999);
    assert_eq!(h.snapshot().analysis.unwrap(), frozen);
}

#[test]
fn analysis_conflict_wait_restores_prior_operation_even_when_paused() {
    let clock = Arc::new(TestClock::default());
    let h = Arc::new(handle(JobKind::Transfer).with_analysis_clock(clock.clone()));
    let queue = ConflictQueue::new();
    queue.join(&h);
    h.start_work();
    let (returned, received) = mpsc::channel();
    let worker = h.clone();
    let worker_clock = clock.clone();
    let thread = std::thread::spawn(move || {
        let _phase = worker.analysis_phase(AnalysisPhase::Copy);
        worker_clock.advance(2);
        worker
            .request_decision(&ConflictInfo {
                source_path: "not-persisted".into(),
                destination_path: "not-persisted".into(),
                source_hash: "not-persisted".into(),
                destination_hash: "not-persisted".into(),
            })
            .unwrap_or_else(|_| panic!("conflict wait was cancelled"));
        returned
            .send(worker.snapshot().analysis.unwrap().phase)
            .unwrap();
    });
    wait(|| h.snapshot().state == JobState::AwaitingDecision);
    clock.advance(3);
    h.set_paused(true);
    clock.advance(5);
    let pending = h.snapshot().pending_conflict.unwrap();
    h.resolve(&pending.request_id, ConflictDecision::Skip)
        .unwrap();
    assert_eq!(
        received.recv_timeout(Duration::from_secs(5)).unwrap(),
        AnalysisPhase::Paused
    );
    thread.join().unwrap();
    clock.advance(7);
    h.set_paused(false);
    let m = metrics(&h);
    assert_eq!(m.copy_secs, 2.0);
    assert_eq!(m.decision_secs, 8.0);
    assert_eq!(m.paused_secs, 7.0);
    assert_eq!(h.snapshot().analysis.unwrap().phase, AnalysisPhase::Other);
    // Without the extra pause, returning from the same wait restores the live copy phase.
    let _phase = h.analysis_phase(AnalysisPhase::Copy);
    h.update(|j| j.state = JobState::AwaitingDecision);
    clock.advance(4);
    h.update(|j| j.state = JobState::Running);
    assert_eq!(h.snapshot().analysis.unwrap().phase, AnalysisPhase::Copy);
}

#[test]
fn analysis_periodic_checkpoints_are_durable_but_live_is_not_summary_history() {
    let store = Arc::new(Store::open_in_memory().unwrap());
    let recorder = HistoryRecorder::new(store.clone());
    let clock = Arc::new(TestClock::default());
    let h = JobHandle::new(
        TransferJob::new(
            "checkpoint".into(),
            "flow".into(),
            "".into(),
            JobKind::Transfer,
        ),
        Arc::new(|| {}),
    )
    .with_analysis(Some(context()), Some(recorder))
    .with_analysis_clock(clock.clone());
    h.record_enqueue();
    h.start_work();
    wait(|| page(&store).jobs[0].state == AnalysisState::Running);
    {
        let _phase = h.analysis_phase(AnalysisPhase::Copy);
        h.analysis_metrics(|m| m.copy_bytes = 123);
        clock.advance(4);
        h.snapshot();
        assert_eq!(page(&store).jobs[0].metrics.copy_bytes, 0);
        clock.advance(1);
        h.snapshot();
        wait(|| page(&store).jobs[0].metrics.copy_bytes == 123);
    }
    let summary = store
        .get_speed_analysis(&AnalysisFilter::default())
        .unwrap();
    assert!(summary.pairs.is_empty());
    h.update(|j| j.state = JobState::Done);
    let saved = page(&store).jobs.remove(0);
    assert_eq!(saved.metrics.copy_secs, 5.0);
    assert_eq!(saved.state, AnalysisState::Done);
    clock.advance(100);
    assert_eq!(h.snapshot().analysis.unwrap(), saved);
}

#[test]
fn analysis_standalone_checks_measure_actual_both_sides_not_ui_budgets() {
    let fx = Fixture::new();
    let bytes = b"photo";
    fx.write_card_file("DCIM/A.JPG", bytes);
    crate::testing::write(fx.nas_dir.path(), "photo/Trip/Camera A Card 1/A.JPG", bytes);
    let h = handle(JobKind::Check);
    h.start_work();
    run_workspace_check(
        &fx.store,
        &fx.resolver,
        &WorkspaceContext::for_project(&fx.store, "project").unwrap(),
        "flow",
        &h,
    )
    .unwrap();
    h.update(|j| j.state = JobState::Done);
    let m = metrics(&h);
    assert_eq!(m.source_check_bytes, 5);
    assert_eq!(m.destination_check_bytes, 5);
    assert_eq!(m.copy_bytes + m.committed_bytes, 0);
    assert_eq!(h.snapshot().bytes_done, 5);
    assert_eq!(h.snapshot().analysis.unwrap().kind, AnalysisKind::Check);

    // The first check creates a source claim. With no destination, only that source is read.
    std::fs::remove_file(fx.nas_dir.path().join("photo/Trip/Camera A Card 1/A.JPG")).unwrap();
    let missing = handle(JobKind::Check);
    missing.start_work();
    run_workspace_check(
        &fx.store,
        &fx.resolver,
        &WorkspaceContext::for_project(&fx.store, "project").unwrap(),
        "flow",
        &missing,
    )
    .unwrap();
    assert_eq!(metrics(&missing).source_check_bytes, 5);
    assert_eq!(metrics(&missing).destination_check_bytes, 0);
    assert_eq!(missing.snapshot().bytes_done, 5);

    // A destination directory cannot be hashed, and the error is telemetry, not a path snapshot.
    std::fs::create_dir(fx.nas_dir.path().join("photo/Trip/Camera A Card 1/A.JPG")).unwrap();
    let broken = handle(JobKind::Check);
    broken.start_work();
    run_workspace_check(
        &fx.store,
        &fx.resolver,
        &WorkspaceContext::for_project(&fx.store, "project").unwrap(),
        "flow",
        &broken,
    )
    .unwrap();
    broken.update(|j| j.state = JobState::Done);
    assert_eq!(broken.snapshot().analysis.unwrap().error_count, 1);
}

#[test]
fn analysis_context_uses_stable_endpoint_tuple_and_name_snapshots() {
    let fx = Fixture::new();
    let context = WorkspaceContext::for_project(&fx.store, "project").unwrap();
    let flow = resolve_workspace_flow(&fx.store, &fx.resolver, &context, "flow").unwrap();
    let original = AnalysisContext::from_flow(&flow);
    assert_eq!(original.pair_id, r#"["space","src","dst","card","nas"]"#);
    let mut source = fx.source.clone();
    source.task_name = "renamed".into();
    fx.store.put(&source).unwrap();
    let all_projects = WorkspaceContext {
        space_id: fx.space.id.clone(),
        project_id: None,
    };
    let flow = resolve_workspace_flow(&fx.store, &fx.resolver, &all_projects, "flow").unwrap();
    let renamed = AnalysisContext::from_flow(&flow);
    assert_eq!(original.pair_id, renamed.pair_id);
    assert_ne!(original.source_name, renamed.source_name);
    assert_eq!(original.source_name, fx.card.name);
    source.device_id = fx.nas.id.clone();
    fx.store.put(&source).unwrap();
    let flow = resolve_workspace_flow(&fx.store, &fx.resolver, &all_projects, "flow").unwrap();
    assert_ne!(original.pair_id, AnalysisContext::from_flow(&flow).pair_id);
}

#[test]
fn analysis_history_survives_recent_pruning_and_clearing_without_wipe_records() {
    let store = Arc::new(Store::open_in_memory().unwrap());
    let (power, _) = super::power::tests::controller(false);
    let manager = TransferManager::with_history(|_| {}, power, Some(store.clone()));
    for i in 0..24 {
        manager.enqueue_with_analysis(
            JobSpec {
                key: format!("f{i}"),
                label: "".into(),
                resources: vec![],
                kind: JobKind::Transfer,
                queue: None,
                work: Box::new(|h| {
                    h.analysis_metrics(|m| {
                        m.copy_bytes = 10;
                        m.committed_bytes = 8;
                    });
                    Ok(())
                }),
            },
            Some(context()),
        );
        wait(|| {
            store
                .get_speed_analysis(&AnalysisFilter::default())
                .unwrap()
                .totals
                .completed_transfer_jobs
                == i + 1
        });
    }
    assert!(manager.jobs().len() < 24);
    wait(|| {
        manager.clear_finished();
        manager.jobs().is_empty()
    });
    assert_eq!(page(&store).total, 24);
    manager.enqueue_with_analysis(
        JobSpec {
            key: "wipe".into(),
            label: "".into(),
            resources: vec![],
            kind: JobKind::Wipe,
            queue: None,
            work: Box::new(|h| {
                assert!(h.snapshot().analysis.is_none());
                Ok(())
            }),
        },
        Some(context()),
    );
    wait(|| !manager.is_busy());
    assert_eq!(page(&store).total, 24);
    assert_eq!(
        store
            .get_speed_analysis(&AnalysisFilter::default())
            .unwrap()
            .totals
            .metrics
            .committed_bytes,
        24 * 8
    );
}

#[test]
fn analysis_queued_cancellation_and_concurrent_live_jobs_stay_out_of_totals() {
    let store = Arc::new(Store::open_in_memory().unwrap());
    let (power, _) = super::power::tests::controller(false);
    let manager = TransferManager::with_history(|_| {}, power, Some(store.clone()));
    let mut releases = vec![];
    for i in 0..2 {
        let (release, held) = mpsc::channel();
        let (started, ready) = mpsc::channel();
        releases.push(release);
        manager.enqueue_with_analysis(
            JobSpec {
                key: format!("running{i}"),
                label: "".into(),
                kind: JobKind::Transfer,
                queue: None,
                resources: vec![ResourceClaim::exclusive(format!("device{i}"))],
                work: Box::new(move |h| {
                    h.analysis_metrics(|m| m.copy_bytes = 99);
                    started.send(()).unwrap();
                    held.recv_timeout(Duration::from_secs(5))
                        .map_err(|e| e.to_string())
                }),
            },
            Some(context()),
        );
        ready.recv_timeout(Duration::from_secs(5)).unwrap();
    }
    let queued = manager.enqueue_with_analysis(
        JobSpec {
            key: "queued".into(),
            label: "".into(),
            kind: JobKind::Transfer,
            queue: None,
            resources: vec![ResourceClaim::exclusive("device0")],
            work: Box::new(|_| panic!("cancelled queued job must not run")),
        },
        Some(context()),
    );
    assert_eq!(
        manager
            .jobs()
            .iter()
            .filter(|j| j.state == JobState::Running)
            .count(),
        2
    );
    assert_eq!(
        manager
            .jobs()
            .iter()
            .find(|j| j.id == queued)
            .unwrap()
            .state,
        JobState::Queued
    );
    assert!(store
        .get_speed_analysis(&AnalysisFilter::default())
        .unwrap()
        .pairs
        .is_empty());
    manager.cancel(&queued);
    let summary = store
        .get_speed_analysis(&AnalysisFilter::default())
        .unwrap();
    assert_eq!(summary.totals.cancelled_jobs, 1);
    assert_eq!(summary.totals.metrics.copy_bytes, 0);
    for release in releases {
        release.send(()).unwrap();
    }
    wait(|| {
        store
            .get_speed_analysis(&AnalysisFilter::default())
            .unwrap()
            .totals
            .completed_transfer_jobs
            == 2
    });
    assert_eq!(
        store
            .get_speed_analysis(&AnalysisFilter::default())
            .unwrap()
            .totals
            .metrics
            .copy_bytes,
        198
    );
}

#[test]
fn analysis_transfer_planning_catalog_work_and_errors_are_preserved() {
    let fx = Fixture::new();
    fx.write_card_file("DCIM/A.JPG", b"photo");
    let h = handle(JobKind::Transfer);
    h.start_work();
    let failures = Mutex::default();
    run_transfer(&fx.store, &fx.resolver, "project", "flow", &h, &failures).unwrap();
    h.update(|j| j.state = JobState::Done);
    let job = h.snapshot();
    assert_eq!(job.bytes_done, 5);
    assert_eq!(job.analysis.unwrap().metrics.committed_bytes, 5);
    assert_eq!(fx.store.list::<crate::domain::FileCopy>().unwrap().len(), 2);
    let frozen = metrics(&h);
    h.update(|j| j.errors.push("too late".into()));
    assert_eq!(metrics(&h), frozen);
}

#[test]
fn analysis_only_snapshots_do_not_feed_progress_or_speed_estimators() {
    let clock = Arc::new(TestClock::default());
    let h = handle(JobKind::Transfer).with_analysis_clock(clock.clone());
    h.start_work();
    {
        let mut job = h.job.lock();
        job.bytes_done = 99;
        job.files_done = 2;
        job.speed_bps = 123;
        job.bytes_per_sec = Some(123);
        job.eta_secs = Some(11);
    }
    clock.advance(3);
    let snapshot = h.telemetry_snapshot();
    assert_eq!(snapshot.bytes_done, 99);
    assert_eq!(snapshot.files_done, 2);
    assert_eq!(snapshot.speed_bps, 123);
    assert_eq!(snapshot.bytes_per_sec, Some(123));
    assert_eq!(snapshot.eta_secs, Some(11));
    assert_eq!(snapshot.analysis.as_ref().unwrap().metrics.other_secs, 3.0);
    assert_eq!(snapshot.analysis.unwrap().metrics.copy_bytes, 0);
}

#[test]
fn analysis_app_enqueue_wires_transfer_check_context_and_durable_history() {
    let fx = Fixture::new();
    fx.write_card_file("DCIM/A.JPG", b"photo");
    let mut settings = crate::app::AppSettings::load(&fx.store);
    settings.keep_awake_during_transfers = false;
    settings.save(&fx.store).unwrap();
    let thumbnails = tempfile::tempdir().unwrap();
    let app = crate::app::AppCore::new(
        fx.store.clone(),
        Arc::new(crate::testing::MapResolver(Mutex::new(
            fx.resolver.0.lock().clone(),
        ))),
        thumbnails.path().into(),
        |_| {},
    );
    let context = WorkspaceContext::for_project(&fx.store, "project").unwrap();
    let transfer_id = app.run_workspace_flow(&context, "flow").unwrap();
    wait(|| {
        fx.store
            .get_speed_analysis(&AnalysisFilter::default())
            .unwrap()
            .totals
            .completed_transfer_jobs
            == 1
    });
    let old = page(&fx.store).jobs.remove(0);
    assert_eq!(old.id, transfer_id);
    assert_eq!(old.context.pair_id, r#"["space","src","dst","card","nas"]"#);
    assert_eq!(old.metrics.copy_bytes, 5);
    assert_eq!(old.metrics.committed_bytes, 5);
    assert_eq!(old.metrics.source_check_bytes, 0);
    assert_eq!(old.metrics.destination_check_bytes, 5);

    let mut source = fx.source.clone();
    source.task_name = "Renamed source".into();
    fx.store.put(&source).unwrap();
    let ids = app.check_workspace_destination(&context, "dst").unwrap();
    wait(|| {
        fx.store
            .get_speed_analysis(&AnalysisFilter::default())
            .unwrap()
            .totals
            .completed_check_jobs
            == 1
    });
    let saved = page(&fx.store);
    let check = saved.jobs.iter().find(|j| j.id == ids[0]).unwrap();
    assert_eq!(check.context.pair_id, old.context.pair_id);
    assert_eq!(check.context.source_name, "Renamed source");
    assert_eq!(check.metrics.source_check_bytes, 5);
    assert_eq!(check.metrics.destination_check_bytes, 5);
    assert_eq!(check.metrics.copy_bytes, 0);
    let summary = fx
        .store
        .get_speed_analysis(&AnalysisFilter::default())
        .unwrap();
    assert_eq!(summary.pairs.len(), 1);
    assert_eq!(summary.pairs[0].context.source_name, "Renamed source");
    assert_eq!(summary.totals.metrics.committed_bytes, 5);
    assert_eq!(summary.totals.metrics.destination_check_bytes, 10);
    assert_eq!(
        saved
            .jobs
            .iter()
            .find(|j| j.id == transfer_id)
            .unwrap()
            .context,
        old.context
    );
}
