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
            errors: vec![],
        },
        Arc::new(|| {}),
    )
}

fn run(fx: &Fixture) -> (JobHandle, Mutex<FailureMap>) {
    let (h, failures) = (handle(), Mutex::new(FailureMap::new()));
    run_transfer(&fx.store, &fx.resolver, &fx.project.id, &fx.flow.id, &h, &failures).unwrap();
    (h, failures)
}

fn categories(fx: &Fixture) -> Vec<(String, Category)> {
    let ctx = crate::plan::resolve_flow(&fx.store, &fx.resolver, &fx.project.id, &fx.flow.id).unwrap();
    let catalog = Catalog::load(&fx.store).unwrap();
    classify_flow(&ctx, &catalog, None).into_iter().map(|f| (f.rel_path, f.category)).collect()
}

#[test]
fn copies_files_and_records_lineage() {
    let fx = Fixture::new();
    fx.write_card_file("DCIM/100/A.JPG", b"photo a");
    fx.write_card_file("DCIM/100/B.MP4", b"video b");
    let (h, _) = run(&fx);
    let job = h.snapshot();
    assert_eq!((job.files_done, job.files_total, job.bytes_done), (2, 2, 14));
    let target = fx.nas_dir.path().join("photo/Iceland/A7IV Card 1/100/A.JPG");
    assert_eq!(std::fs::read(target).unwrap(), b"photo a");
    let copies = fx.store.list::<FileCopy>().unwrap();
    assert_eq!(copies.len(), 4);
    assert!(copies.iter().any(|c| c.device_id == "card" && c.path == "DCIM/100/A.JPG"));
    assert!(categories(&fx).iter().all(|(_, c)| *c == Category::Transferred));
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
fn adopts_identical_and_renames_conflicting_targets() {
    let fx = Fixture::new();
    fx.write_card_file("DCIM/A.JPG", b"same");
    fx.write_card_file("DCIM/B.JPG", b"new content");
    let dest = fx.nas_dir.path().join("photo/Iceland/A7IV Card 1");
    write(&dest, "A.JPG", b"same");
    write(&dest, "B.JPG", b"other file");
    run(&fx);
    assert_eq!(std::fs::read(dest.join("B.JPG")).unwrap(), b"other file");
    assert_eq!(std::fs::read(dest.join("B (1).JPG")).unwrap(), b"new content");
    assert!(!dest.join("A (1).JPG").exists());
}

#[test]
fn reread_verification_and_no_partials_left() {
    let mut fx = Fixture::new();
    fx.space.verify_mode = VerifyMode::Reread;
    fx.store.put(&fx.space).unwrap();
    fx.write_card_file("DCIM/A.JPG", &vec![7u8; 9_000_000]);
    run(&fx);
    let dest = fx.nas_dir.path().join("photo/Iceland/A7IV Card 1");
    let names: Vec<_> = std::fs::read_dir(&dest).unwrap().map(|e| e.unwrap().file_name()).collect();
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
    assert!(fx.nas_dir.path().join("backups/card-Iceland/A.JPG").exists());
    assert_eq!(crate::paths::read_backup_folder(fx.card_dir.path()).as_deref(), Some("card-Iceland"));
}

#[test]
fn offline_destination_fails_cleanly() {
    let fx = Fixture::new();
    fx.unmount("nas");
    let err = run_transfer(&fx.store, &fx.resolver, "project", "flow", &handle(), &Mutex::new(FailureMap::new()));
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
        devices: vec!["d".into()],
        work: Box::new(|h| {
            for _ in 0..5 {
                h.checkpoint().map_err(|_| "cancelled".to_string())?;
                std::thread::sleep(Duration::from_millis(20));
            }
            Ok(())
        }),
    });
    assert_eq!(manager.enqueue(JobSpec { key: "k".into(), label: String::new(), devices: vec![], work: Box::new(|_| Ok(())) }), id);
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
fn manager_serialises_jobs_sharing_a_device() {
    let manager = TransferManager::new(|_| {});
    let order = Arc::new(Mutex::new(Vec::new()));
    for n in 0..2 {
        let order = order.clone();
        manager.enqueue(JobSpec {
            key: format!("job{n}"),
            label: String::new(),
            devices: vec!["shared".into()],
            work: Box::new(move |_| {
                order.lock().push(format!("start{n}"));
                std::thread::sleep(Duration::from_millis(50));
                order.lock().push(format!("end{n}"));
                Ok(())
            }),
        });
    }
    while manager.is_busy() {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(*order.lock(), vec!["start0", "end0", "start1", "end1"]);
}
