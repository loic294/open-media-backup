use super::handle::{ConflictDecision, ConflictQueue, JobHandle, JobKind, JobState, TransferJob};
use super::history::HistoryRecorder;
use super::power::PowerController;
use super::AnalysisContext;
use crate::domain::new_id;
use crate::store::Store;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};
use std::time::Duration;

const EMIT_INTERVAL: Duration = Duration::from_millis(200);
const KEEP_FINISHED: usize = 20;
const MAX_CONCURRENT_JOBS: usize = 3;

pub type Work = Box<dyn FnOnce(&JobHandle) -> Result<(), String> + Send>;

/// A resource that can be shared by concurrent jobs or exclusively claimed by one job.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourceClaim {
    key: String,
    exclusive: bool,
}

impl ResourceClaim {
    pub fn shared(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            exclusive: false,
        }
    }

    pub fn exclusive(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            exclusive: true,
        }
    }
}

/// A unit of background work with resource claims used to prevent unsafe overlap.
pub struct JobSpec {
    pub key: String,
    pub label: String,
    pub resources: Vec<ResourceClaim>,
    pub work: Work,
    pub kind: JobKind,
    /// Conflict decisions are shared by all jobs of one queue.
    pub queue: Option<Arc<ConflictQueue>>,
}

struct Entry {
    handle: Arc<JobHandle>,
    resources: Vec<ResourceClaim>,
    work: Option<Work>,
    ready: bool,
}

struct Inner {
    entries: Mutex<Vec<Entry>>,
    dirty: AtomicBool,
    on_change: Box<dyn Fn(Vec<TransferJob>) + Send + Sync>,
    power: PowerController,
    history: Option<HistoryRecorder>,
}

#[derive(Clone)]
pub struct TransferManager {
    inner: Arc<Inner>,
}

impl TransferManager {
    pub fn new(on_change: impl Fn(Vec<TransferJob>) + Send + Sync + 'static) -> Self {
        Self::with_power_controller(
            on_change,
            PowerController::new(true, |warning| log::warn!("{warning}")),
        )
    }

    pub(crate) fn with_power_controller(
        on_change: impl Fn(Vec<TransferJob>) + Send + Sync + 'static,
        power: PowerController,
    ) -> Self {
        Self::with_history(on_change, power, None)
    }

    pub(crate) fn with_history(
        on_change: impl Fn(Vec<TransferJob>) + Send + Sync + 'static,
        power: PowerController,
        store: Option<Arc<Store>>,
    ) -> Self {
        let inner = Arc::new(Inner {
            entries: Mutex::new(vec![]),
            dirty: AtomicBool::new(false),
            on_change: Box::new(on_change),
            power,
            history: store.map(HistoryRecorder::new),
        });
        spawn_emitter(Arc::downgrade(&inner));
        Self { inner }
    }

    pub(crate) fn set_keep_awake(&self, enabled: bool) {
        self.inner.power.set_enabled(enabled);
    }

    /// Queues a job, or returns the id of the unfinished job with the same key.
    pub fn enqueue(&self, spec: JobSpec) -> String {
        self.enqueue_with_analysis(spec, None)
    }

    pub fn enqueue_with_analysis(&self, spec: JobSpec, context: Option<AnalysisContext>) -> String {
        let mut entries = self.inner.entries.lock();
        if let Some(existing) = entries.iter().find(|e| {
            let job = e.handle.snapshot();
            job.flow_id == spec.key && !job.state.is_finished()
        }) {
            return existing.handle.snapshot().id;
        }
        let id = new_id();
        let job = TransferJob::new(id.clone(), spec.key, spec.label, spec.kind);
        let weak = Arc::downgrade(&self.inner);
        let notify = Arc::new(move || {
            if let Some(inner) = weak.upgrade() {
                inner.dirty.store(true, Ordering::SeqCst);
            }
        });
        let handle = Arc::new(
            JobHandle::with_power(job, notify, self.inner.power.clone())
                .with_analysis(context, self.inner.history.clone()),
        );
        if let Some(queue) = &spec.queue {
            queue.join(&handle);
        }
        entries.push(Entry {
            handle: handle.clone(),
            resources: spec.resources,
            work: Some(spec.work),
            ready: false,
        });
        prune(&mut entries);
        drop(entries);
        handle.record_enqueue();
        if let Some(entry) = self
            .inner
            .entries
            .lock()
            .iter_mut()
            .find(|entry| Arc::ptr_eq(&entry.handle, &handle))
        {
            entry.ready = true;
        }
        self.schedule();
        self.inner.dirty.store(true, Ordering::SeqCst);
        id
    }

    pub fn analysis_storage_error(&self) -> Option<String> {
        self.inner.history.as_ref().and_then(HistoryRecorder::error)
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

    /// Prevents enqueue/scheduling races while synchronously updating an idle device's catalog.
    pub(crate) fn with_idle_resource<T>(
        &self,
        key: &str,
        work: impl FnOnce() -> Result<T, String>,
    ) -> Result<T, String> {
        let entries = self.inner.entries.lock();
        if entries.iter().any(|entry| {
            !entry.handle.snapshot().state.is_finished()
                && entry.resources.iter().any(|resource| resource.key == key)
        }) {
            return Err(
                "Finish or cancel this device's active jobs before marking it manually wiped"
                    .into(),
            );
        }
        let result = work();
        drop(entries);
        result
    }

    pub fn set_paused(&self, id: &str, paused: bool) {
        self.with_handle(id, |h| h.set_paused(paused));
    }

    pub fn set_all_paused(&self, paused: bool) {
        let handles: Vec<_> = self
            .inner
            .entries
            .lock()
            .iter()
            .map(|e| e.handle.clone())
            .collect();
        for handle in handles {
            handle.set_paused(paused);
        }
    }

    pub fn cancel(&self, id: &str) {
        self.with_handle(id, |h| h.cancel());
        let handle = {
            let mut entries = self.inner.entries.lock();
            entries
                .iter_mut()
                .find(|e| e.handle.snapshot().id == id && e.work.is_some())
                .map(|entry| {
                    entry.work = None;
                    entry.handle.clone()
                })
        };
        if let Some(handle) = handle {
            handle.leave_queue();
            handle.update(|j| j.state = JobState::Cancelled);
        }
    }

    /// Answers a job's pending conflict. With `apply_to_remaining`, every waiting and
    /// future conflict of the same queue gets the same decision.
    pub fn resolve_conflict(
        &self,
        job_id: &str,
        request_id: &str,
        decision: ConflictDecision,
        apply_to_remaining: bool,
    ) -> Result<(), String> {
        let handle = self
            .inner
            .entries
            .lock()
            .iter()
            .find(|e| e.handle.snapshot().id == job_id)
            .map(|e| e.handle.clone())
            .ok_or("Transfer not found")?;
        handle.resolve(request_id, decision)?;
        if apply_to_remaining {
            if let Some(queue) = handle.queue() {
                queue.apply_to_remaining(decision);
            }
        }
        self.inner.dirty.store(true, Ordering::SeqCst);
        Ok(())
    }

    pub fn clear_finished(&self) {
        self.inner.entries.lock().retain(|e| {
            !e.handle.snapshot().state.is_finished()
                || !e.handle.final_recorded.load(Ordering::SeqCst)
        });
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

    /// Starts queued jobs while respecting the worker limit and resource claims.
    fn schedule(&self) {
        let mut entries = self.inner.entries.lock();
        let mut active_resources: Vec<ResourceClaim> = entries
            .iter()
            .filter(|e| e.work.is_none() && !e.handle.snapshot().state.is_finished())
            .flat_map(|e| e.resources.iter().cloned())
            .collect();
        let mut active = entries
            .iter()
            .filter(|e| e.work.is_none() && !e.handle.snapshot().state.is_finished())
            .count();
        for entry in entries.iter_mut() {
            if active >= MAX_CONCURRENT_JOBS {
                break;
            }
            if !entry.ready
                || entry.work.is_none()
                || entry
                    .resources
                    .iter()
                    .any(|claim| conflicts_with_active(claim, &active_resources))
            {
                continue;
            }
            active_resources.extend(entry.resources.iter().cloned());
            active += 1;
            let work = entry.work.take().expect("checked");
            let handle = entry.handle.clone();
            let manager = self.clone();
            std::thread::spawn(move || {
                handle.start_work();
                let result =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| work(&handle)))
                        .unwrap_or_else(|panic| {
                            let message = panic
                                .downcast_ref::<String>()
                                .map(String::as_str)
                                .or_else(|| panic.downcast_ref::<&str>().copied())
                                .unwrap_or("unknown panic");
                            log::error!("Background transfer job panicked: {message}");
                            Err(format!("Background job panicked: {message}"))
                        });
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
                handle.leave_queue();
                manager.schedule();
            });
        }
    }
}

fn conflicts_with_active(claim: &ResourceClaim, active: &[ResourceClaim]) -> bool {
    (claim.exclusive && active.iter().any(|other| other.key == claim.key))
        || active
            .iter()
            .any(|other| other.exclusive && other.key == claim.key)
}

fn prune(entries: &mut Vec<Entry>) {
    let finished = entries
        .iter()
        .filter(|e| e.handle.snapshot().state.is_finished())
        .count();
    let mut excess = finished.saturating_sub(KEEP_FINISHED);
    entries.retain(|e| {
        if excess > 0
            && e.handle.snapshot().state.is_finished()
            && e.handle.final_recorded.load(Ordering::SeqCst)
        {
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
        let dirty = inner.dirty.swap(false, Ordering::SeqCst);
        let jobs: Vec<_> = inner
            .entries
            .lock()
            .iter()
            .map(|e| {
                if dirty {
                    e.handle.snapshot()
                } else {
                    e.handle.telemetry_snapshot()
                }
            })
            .collect();
        if dirty
            || jobs
                .iter()
                .any(|job| job.analysis.is_some() && !job.state.is_finished())
        {
            (inner.on_change)(jobs);
        }
    });
}
