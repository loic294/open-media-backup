use super::power::tests::controller;
use super::power::PowerController;
use super::*;
use parking_lot::Mutex;
use std::sync::atomic::Ordering;
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

fn wait_idle(manager: &TransferManager) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while manager.is_busy() {
        assert!(Instant::now() < deadline, "jobs did not finish");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn held_job(
    manager: &TransferManager,
    key: &str,
    kind: JobKind,
    resource: &str,
) -> (String, mpsc::Receiver<()>, mpsc::Sender<()>) {
    let (started, start) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let id = manager.enqueue(JobSpec {
        key: key.into(),
        label: key.into(),
        resources: vec![ResourceClaim::exclusive(resource)],
        kind,
        queue: None,
        work: Box::new(move |_| {
            started.send(()).map_err(|error| error.to_string())?;
            gate.recv_timeout(Duration::from_secs(5))
                .map_err(|error| error.to_string())
        }),
    });
    (id, start, release)
}

#[test]
fn concurrent_transfers_share_protection_and_live_settings_reconcile() {
    let (power, probe) = controller(true);
    let manager = TransferManager::with_power_controller(|_| {}, power);
    let (a, started_a, release_a) = held_job(&manager, "a", JobKind::Transfer, "a");
    let (b, started_b, release_b) = held_job(&manager, "b", JobKind::Transfer, "b");
    started_a.recv_timeout(Duration::from_secs(5)).unwrap();
    started_b.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(probe.acquired.load(Ordering::SeqCst), 1);
    manager.set_paused(&a, true);
    assert_eq!(probe.released.load(Ordering::SeqCst), 0);
    manager.set_paused(&b, true);
    assert_eq!(probe.released.load(Ordering::SeqCst), 1);
    manager.set_paused(&a, false);
    assert_eq!(probe.acquired.load(Ordering::SeqCst), 2);
    manager.set_keep_awake(false);
    assert_eq!(probe.released.load(Ordering::SeqCst), 2);
    manager.set_keep_awake(true);
    assert_eq!(probe.acquired.load(Ordering::SeqCst), 3);
    release_a.send(()).unwrap();
    release_b.send(()).unwrap();
    wait_idle(&manager);
    assert_eq!(probe.released.load(Ordering::SeqCst), 3);
}

#[test]
fn queued_pause_resume_and_check_wipe_jobs_do_not_acquire_protection() {
    let (power, probe) = controller(true);
    let manager = TransferManager::with_power_controller(|_| {}, power);
    let (_, started, release) = held_job(&manager, "check", JobKind::Check, "shared");
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    let (id, transfer_started, release_transfer) =
        held_job(&manager, "transfer", JobKind::Transfer, "shared");
    manager.set_paused(&id, true);
    manager.set_paused(&id, false);
    assert_eq!(probe.acquired.load(Ordering::SeqCst), 0);
    manager.set_paused(&id, true);
    release.send(()).unwrap();
    transfer_started
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    assert_eq!(probe.acquired.load(Ordering::SeqCst), 0);
    manager.set_paused(&id, false);
    assert_eq!(
        manager
            .jobs()
            .iter()
            .find(|job| job.id == id)
            .unwrap()
            .state,
        JobState::Running
    );
    assert_eq!(probe.acquired.load(Ordering::SeqCst), 1);
    release_transfer.send(()).unwrap();
    wait_idle(&manager);
    assert_eq!(probe.released.load(Ordering::SeqCst), 1);
    let (_, wipe_started, wipe_release) = held_job(&manager, "wipe", JobKind::Wipe, "shared");
    wipe_started.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(probe.acquired.load(Ordering::SeqCst), 1);
    wipe_release.send(()).unwrap();
    wait_idle(&manager);
}

#[test]
fn cancellation_keeps_protection_until_work_stops_and_queued_cancel_never_acquires() {
    let (power, probe) = controller(true);
    let manager = TransferManager::with_power_controller(|_| {}, power);
    let (id, started, release) = held_job(&manager, "transfer", JobKind::Transfer, "shared");
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    let (queued, _, _) = held_job(&manager, "queued", JobKind::Transfer, "shared");
    manager.cancel(&queued);
    manager.cancel(&id);
    assert_eq!(probe.acquired.load(Ordering::SeqCst), 1);
    assert_eq!(probe.released.load(Ordering::SeqCst), 0);
    release.send(()).unwrap();
    wait_idle(&manager);
    assert_eq!(probe.released.load(Ordering::SeqCst), 1);
    assert!(manager
        .jobs()
        .iter()
        .all(|job| job.state == JobState::Cancelled));
}

#[test]
fn conflict_wait_releases_and_resolution_reacquires_before_work_continues() {
    let (power, probe) = controller(true);
    let manager = TransferManager::with_power_controller(|_| {}, power);
    let (resumed, continuation) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let id = manager.enqueue(JobSpec {
        key: "conflict".into(),
        label: "conflict".into(),
        resources: vec![],
        kind: JobKind::Transfer,
        queue: Some(ConflictQueue::new()),
        work: Box::new(move |handle| {
            handle
                .request_decision(&ConflictInfo {
                    source_path: "card/photo.jpg".into(),
                    destination_path: "backup/photo.jpg".into(),
                    source_hash: "source".into(),
                    destination_hash: "different".into(),
                })
                .map_err(|_| "cancelled".to_string())?;
            resumed.send(()).map_err(|error| error.to_string())?;
            gate.recv_timeout(Duration::from_secs(5))
                .map_err(|error| error.to_string())
        }),
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    let request = loop {
        if let Some(request) = manager.jobs()[0].pending_conflict.clone() {
            break request;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    };
    assert_eq!(probe.acquired.load(Ordering::SeqCst), 1);
    assert_eq!(probe.released.load(Ordering::SeqCst), 1);
    manager
        .resolve_conflict(&id, &request.request_id, ConflictDecision::KeepBoth, false)
        .unwrap();
    continuation.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(probe.acquired.load(Ordering::SeqCst), 2);
    release.send(()).unwrap();
    wait_idle(&manager);
    assert_eq!(probe.released.load(Ordering::SeqCst), 2);
}

#[test]
fn verifying_and_terminal_states_reconcile_and_dropped_handles_release() {
    let (power, probe) = controller(true);
    for terminal in [JobState::Done, JobState::Failed, JobState::Cancelled] {
        let handle = JobHandle::with_power(
            TransferJob::new(
                "job".into(),
                "flow".into(),
                "test".into(),
                JobKind::Transfer,
            ),
            Arc::new(|| {}),
            power.clone(),
        );
        handle.start_work();
        let acquired = probe.acquired.load(Ordering::SeqCst);
        handle.update(|job| job.state = JobState::Verifying);
        handle.add_bytes(1);
        assert_eq!(probe.acquired.load(Ordering::SeqCst), acquired);
        handle.update(|job| job.state = terminal);
        assert_eq!(probe.released.load(Ordering::SeqCst), acquired);
    }

    let handle = JobHandle::with_power(
        TransferJob::new(
            "last".into(),
            "flow".into(),
            "test".into(),
            JobKind::Transfer,
        ),
        Arc::new(|| {}),
        power,
    );
    handle.start_work();
    drop(handle);
    assert_eq!(
        probe.released.load(Ordering::SeqCst),
        probe.acquired.load(Ordering::SeqCst)
    );
}

#[test]
fn cancelling_a_conflict_does_not_reacquire_or_release_scheduling_resources_before_cleanup() {
    let (power, probe) = controller(true);
    let manager = TransferManager::with_power_controller(|_| {}, power);
    let (cleaning, cleanup) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let id = manager.enqueue(JobSpec {
        key: "conflict-cancel".into(),
        label: "conflict".into(),
        resources: vec![ResourceClaim::exclusive("shared")],
        kind: JobKind::Transfer,
        queue: Some(ConflictQueue::new()),
        work: Box::new(move |handle| {
            let result = handle.request_decision(&ConflictInfo {
                source_path: "card/photo.jpg".into(),
                destination_path: "backup/photo.jpg".into(),
                source_hash: "source".into(),
                destination_hash: "different".into(),
            });
            cleaning.send(()).map_err(|error| error.to_string())?;
            gate.recv_timeout(Duration::from_secs(5))
                .map_err(|error| error.to_string())?;
            result.map(|_| ()).map_err(|_| "cancelled".into())
        }),
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    while manager.jobs()[0].pending_conflict.is_none() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    manager.cancel(&id);
    cleanup.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(!manager.jobs()[0].state.is_finished());
    assert_eq!(probe.acquired.load(Ordering::SeqCst), 1);
    assert_eq!(probe.released.load(Ordering::SeqCst), 1);
    release.send(()).unwrap();
    wait_idle(&manager);
    assert_eq!(manager.jobs()[0].state, JobState::Cancelled);
}

#[test]
fn worker_error_and_panic_release_protection_and_do_not_block_next_jobs() {
    let (power, probe) = controller(true);
    let manager = TransferManager::with_power_controller(|_| {}, power);
    for panic in [false, true] {
        manager.enqueue(JobSpec {
            key: format!("failure-{panic}"),
            label: "failure".into(),
            resources: vec![ResourceClaim::exclusive("shared")],
            kind: JobKind::Transfer,
            queue: None,
            work: Box::new(move |_| {
                if panic {
                    panic!("test worker panic");
                }
                Err("test copy failure".into())
            }),
        });
        wait_idle(&manager);
        assert_eq!(
            probe.acquired.load(Ordering::SeqCst),
            probe.released.load(Ordering::SeqCst)
        );
    }
    assert!(manager
        .jobs()
        .iter()
        .all(|job| job.state == JobState::Failed && !job.errors.is_empty()));
}

#[test]
fn os_failure_warns_but_actual_copy_still_finishes_successfully() {
    let warnings = Arc::new(Mutex::new(Vec::new()));
    let sink = warnings.clone();
    let power = PowerController::with_factory(
        true,
        Box::new(|| Err("OS denied sleep prevention".into())),
        Arc::new(move |warning| sink.lock().push(warning)),
    );
    let manager = TransferManager::with_power_controller(|_| {}, power);
    let fixture = crate::testing::Fixture::new();
    fixture.write_card_file("DCIM/PHOTO.JPG", b"photo bytes");
    let store = fixture.store.clone();
    let resolver = fixture.resolver;
    manager.enqueue(JobSpec {
        key: "flow".into(),
        label: "copy".into(),
        resources: vec![],
        kind: JobKind::Transfer,
        queue: None,
        work: Box::new(move |handle| {
            run_transfer(
                &store,
                &resolver,
                "project",
                "flow",
                handle,
                &Mutex::default(),
            )
        }),
    });
    wait_idle(&manager);
    assert_eq!(warnings.lock().len(), 1);
    assert_eq!(manager.jobs()[0].state, JobState::Done);
    assert!(manager.jobs()[0].errors.is_empty());
    assert_eq!(
        std::fs::read(
            fixture
                .nas_dir
                .path()
                .join("photo/Trip/Camera A Card 1/PHOTO.JPG")
        )
        .unwrap(),
        b"photo bytes",
    );
}

#[test]
fn saving_preferences_updates_active_transfers_and_persists_for_restart() {
    let fixture = crate::testing::Fixture::new();
    let thumbnails = tempfile::tempdir().unwrap();
    let mut core = crate::app::AppCore::new(
        fixture.store.clone(),
        Arc::new(fixture.resolver),
        thumbnails.path().into(),
        |_| {},
    );
    let (power, probe) = controller(false);
    core.transfers = TransferManager::with_power_controller(|_| {}, power);
    let (_, started, release) = held_job(&core.transfers, "transfer", JobKind::Transfer, "a");
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(probe.acquired.load(Ordering::SeqCst), 0);
    let mut settings = core.settings();
    settings.keep_awake_during_transfers = true;
    core.save_settings(&settings).unwrap();
    assert!(crate::app::AppSettings::load(&fixture.store).keep_awake_during_transfers);
    assert_eq!(probe.acquired.load(Ordering::SeqCst), 1);
    settings.keep_awake_during_transfers = false;
    core.save_settings(&settings).unwrap();
    assert!(!crate::app::AppSettings::load(&fixture.store).keep_awake_during_transfers);
    assert_eq!(probe.released.load(Ordering::SeqCst), 1);
    release.send(()).unwrap();
    wait_idle(&core.transfers);
}

#[cfg(target_os = "macos")]
#[test]
#[ignore = "uses native macOS power assertions; run explicitly"]
fn native_macos_transfer_assertion_tracks_pause_settings_and_completion() {
    fn asserted() -> bool {
        let output = std::process::Command::new("/usr/bin/pmset")
            .args(["-g", "assertions"])
            .output()
            .unwrap();
        assert!(output.status.success());
        let pid = format!("pid {}(", std::process::id());
        let output = String::from_utf8_lossy(&output.stdout);
        let assertions: Vec<_> = output
            .lines()
            .filter(|line| line.contains(&pid) && line.contains("Open Media Backup transfer"))
            .collect();
        assert!(
            assertions.iter().all(|line| {
                !line.contains("PreventUserIdleDisplaySleep")
                    && !line.contains("PreventSystemSleep")
            }),
            "must not prevent display or explicit sleep"
        );
        assertions
            .iter()
            .any(|line| line.contains("PreventUserIdleSystemSleep"))
    }

    let warnings = Arc::new(Mutex::new(Vec::new()));
    let sink = warnings.clone();
    let power = PowerController::with_factory(
        true,
        Box::new(super::power::native_guard),
        Arc::new(move |warning| sink.lock().push(warning)),
    );
    let manager = TransferManager::with_power_controller(|_| {}, power);
    let fixture = crate::testing::Fixture::new();
    fixture.write_card_file("DCIM/PHOTO.JPG", b"native power protected transfer");
    let store = fixture.store.clone();
    let resolver = fixture.resolver;
    let (started, start) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let id = manager.enqueue(JobSpec {
        key: "flow".into(),
        label: "native assertion copy".into(),
        resources: vec![],
        kind: JobKind::Transfer,
        queue: None,
        work: Box::new(move |handle| {
            started.send(()).map_err(|error| error.to_string())?;
            gate.recv_timeout(Duration::from_secs(10))
                .map_err(|error| error.to_string())?;
            run_transfer(
                &store,
                &resolver,
                "project",
                "flow",
                handle,
                &Mutex::default(),
            )
        }),
    });
    start.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(warnings.lock().is_empty(), "{:?}", *warnings.lock());
    assert!(asserted());
    manager.set_paused(&id, true);
    assert!(!asserted());
    manager.set_paused(&id, false);
    assert!(asserted());
    manager.set_keep_awake(false);
    assert!(!asserted());
    manager.set_keep_awake(true);
    assert!(asserted());
    release.send(()).unwrap();
    wait_idle(&manager);
    assert!(!asserted());
    assert_eq!(manager.jobs()[0].state, JobState::Done);
    assert_eq!(
        std::fs::read(
            fixture
                .nas_dir
                .path()
                .join("photo/Trip/Camera A Card 1/PHOTO.JPG")
        )
        .unwrap(),
        b"native power protected transfer",
    );
}
