use super::handle::{JobHandle, JobState, TransferJob};
use crate::domain::new_id;
use parking_lot::Mutex;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};
use std::time::Duration;

const EMIT_INTERVAL: Duration = Duration::from_millis(200);
const KEEP_FINISHED: usize = 20;

pub type Work = Box<dyn FnOnce(&JobHandle) -> Result<(), String> + Send>;

/// A unit of background work. Jobs touching the same device never run concurrently.
pub struct JobSpec {
    pub key: String,
    pub label: String,
    pub devices: Vec<String>,
    pub work: Work,
}

struct Entry {
    handle: Arc<JobHandle>,
    devices: Vec<String>,
    work: Option<Work>,
}

struct Inner {
    entries: Mutex<Vec<Entry>>,
    dirty: AtomicBool,
    on_change: Box<dyn Fn(Vec<TransferJob>) + Send + Sync>,
}

#[derive(Clone)]
pub struct TransferManager {
    inner: Arc<Inner>,
}

impl TransferManager {
    pub fn new(on_change: impl Fn(Vec<TransferJob>) + Send + Sync + 'static) -> Self {
        let inner = Arc::new(Inner {
            entries: Mutex::new(vec![]),
            dirty: AtomicBool::new(false),
            on_change: Box::new(on_change),
        });
        spawn_emitter(Arc::downgrade(&inner));
        Self { inner }
    }

    /// Queues a job, or returns the id of the unfinished job with the same key.
    pub fn enqueue(&self, spec: JobSpec) -> String {
        let mut entries = self.inner.entries.lock();
        if let Some(existing) = entries.iter().find(|e| {
            let job = e.handle.snapshot();
            job.flow_id == spec.key && !job.state.is_finished()
        }) {
            return existing.handle.snapshot().id;
        }
        let id = new_id();
        let job = TransferJob {
            id: id.clone(),
            flow_id: spec.key,
            label: spec.label,
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
        };
        let weak = Arc::downgrade(&self.inner);
        let notify = Arc::new(move || {
            if let Some(inner) = weak.upgrade() {
                inner.dirty.store(true, Ordering::SeqCst);
            }
        });
        entries.push(Entry {
            handle: Arc::new(JobHandle::new(job, notify)),
            devices: spec.devices,
            work: Some(spec.work),
        });
        prune(&mut entries);
        drop(entries);
        self.schedule();
        self.inner.dirty.store(true, Ordering::SeqCst);
        id
    }

    pub fn jobs(&self) -> Vec<TransferJob> {
        self.inner
            .entries
            .lock()
            .iter()
            .map(|e| e.handle.snapshot())
            .collect()
    }

    pub fn is_busy(&self) -> bool {
        self.jobs().iter().any(|j| !j.state.is_finished())
    }

    pub fn set_paused(&self, id: &str, paused: bool) {
        self.with_handle(id, |h| h.set_paused(paused));
    }

    pub fn set_all_paused(&self, paused: bool) {
        for entry in self.inner.entries.lock().iter() {
            entry.handle.set_paused(paused);
        }
    }

    pub fn cancel(&self, id: &str) {
        self.with_handle(id, |h| h.cancel());
        let mut entries = self.inner.entries.lock();
        if let Some(entry) = entries
            .iter_mut()
            .find(|e| e.handle.snapshot().id == id && e.work.is_some())
        {
            entry.work = None;
            entry.handle.update(|j| j.state = JobState::Cancelled);
        }
    }

    pub fn clear_finished(&self) {
        self.inner
            .entries
            .lock()
            .retain(|e| !e.handle.snapshot().state.is_finished());
        self.inner.dirty.store(true, Ordering::SeqCst);
    }

    fn with_handle(&self, id: &str, f: impl FnOnce(&JobHandle)) {
        let handle = self
            .inner
            .entries
            .lock()
            .iter()
            .find(|e| e.handle.snapshot().id == id)
            .map(|e| e.handle.clone());
        if let Some(h) = handle {
            f(&h);
        }
    }

    /// Starts every queued job whose devices are not used by a running job.
    fn schedule(&self) {
        let mut entries = self.inner.entries.lock();
        let mut busy: HashSet<String> = entries
            .iter()
            .filter(|e| e.work.is_none() && !e.handle.snapshot().state.is_finished())
            .flat_map(|e| e.devices.clone())
            .collect();
        for entry in entries.iter_mut() {
            if entry.work.is_none() || entry.devices.iter().any(|d| busy.contains(d)) {
                continue;
            }
            busy.extend(entry.devices.iter().cloned());
            let work = entry.work.take().expect("checked");
            let handle = entry.handle.clone();
            let manager = self.clone();
            std::thread::spawn(move || {
                if !handle.is_paused() {
                    handle.update(|j| j.state = JobState::Running);
                }
                let result = work(&handle);
                let cancelled = handle.is_cancelled();
                handle.update(|j| {
                    j.current_file = None;
                    j.state = match (&result, cancelled) {
                        (_, true) => JobState::Cancelled,
                        (Err(e), _) => {
                            j.errors.push(e.clone());
                            JobState::Failed
                        }
                        (Ok(()), _) if !j.errors.is_empty() => JobState::Failed,
                        _ => JobState::Done,
                    };
                });
                manager.schedule();
            });
        }
    }
}

fn prune(entries: &mut Vec<Entry>) {
    let finished = entries
        .iter()
        .filter(|e| e.handle.snapshot().state.is_finished())
        .count();
    let mut excess = finished.saturating_sub(KEEP_FINISHED);
    entries.retain(|e| {
        if excess > 0 && e.handle.snapshot().state.is_finished() {
            excess -= 1;
            return false;
        }
        true
    });
}

fn spawn_emitter(weak: Weak<Inner>) {
    std::thread::spawn(move || loop {
        std::thread::sleep(EMIT_INTERVAL);
        let Some(inner) = weak.upgrade() else { return };
        if inner.dirty.swap(false, Ordering::SeqCst) {
            let jobs = inner
                .entries
                .lock()
                .iter()
                .map(|e| e.handle.snapshot())
                .collect();
            (inner.on_change)(jobs);
        }
    });
}
