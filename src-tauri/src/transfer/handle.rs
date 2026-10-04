use super::speed::SpeedSmoother;
use crate::domain::new_id;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Queued,
    Running,
    Verifying,
    Paused,
    AwaitingDecision,
    Done,
    Failed,
    Cancelled,
}

impl JobState {
    pub fn is_finished(self) -> bool {
        matches!(self, Self::Done | Self::Failed | Self::Cancelled)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobKind {
    #[default]
    Transfer,
    Check,
    Wipe,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictDecision {
    Skip,
    KeepBoth,
    Replace,
}

/// An existing destination file whose content differs from the source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictInfo {
    pub source_path: String,
    pub destination_path: String,
    pub source_hash: String,
    pub destination_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PendingConflict {
    pub request_id: String,
    pub source_path: String,
    pub destination_path: String,
    pub source_hash: String,
    pub destination_hash: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckOutcome {
    Matched,
    Missing,
    Conflict,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CheckItem {
    pub source_path: String,
    pub destination_path: String,
    pub outcome: CheckOutcome,
    pub error: Option<String>,
}

/// Counts cover every inspected file; `items` lists only the files that need attention.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct CheckResults {
    pub matched: usize,
    pub missing: usize,
    pub conflicts: usize,
    pub errors: usize,
    pub items: Vec<CheckItem>,
}

/// Progress snapshot sent to the UI.
#[derive(Debug, Clone, Serialize)]
pub struct TransferJob {
    pub id: String,
    pub flow_id: String,
    pub label: String,
    pub state: JobState,
    pub files_done: usize,
    pub files_total: usize,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub current_file: Option<String>,
    pub speed_bps: u64,
    pub bytes_per_sec: Option<u64>,
    pub eta_secs: Option<u64>,
    pub errors: Vec<String>,
    pub kind: JobKind,
    pub pending_conflict: Option<PendingConflict>,
    pub check_results: Option<CheckResults>,
}

impl TransferJob {
    pub fn new(id: String, flow_id: String, label: String, kind: JobKind) -> Self {
        Self {
            id,
            flow_id,
            label,
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
            kind,
            pending_conflict: None,
            check_results: None,
        }
    }
}

pub struct Cancelled;

/// Decisions shared by the jobs started from one Run/Retry/Run all action.
#[derive(Default)]
pub struct ConflictQueue {
    apply_all: Mutex<Option<ConflictDecision>>,
    members: Mutex<Vec<Weak<JobHandle>>>,
}

impl ConflictQueue {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub(super) fn join(self: &Arc<Self>, handle: &Arc<JobHandle>) {
        self.members.lock().push(Arc::downgrade(handle));
        *handle.queue.lock() = Some(self.clone());
    }

    /// Remembers the decision for later conflicts and answers waiting requests of this queue.
    pub(super) fn apply_to_remaining(&self, decision: ConflictDecision) {
        *self.apply_all.lock() = Some(decision);
        let members: Vec<_> = self
            .members
            .lock()
            .iter()
            .filter_map(Weak::upgrade)
            .collect();
        for member in members {
            member.answer_pending(decision);
        }
    }

    fn preset(&self) -> Option<ConflictDecision> {
        *self.apply_all.lock()
    }
}

struct PendingSlot {
    request_id: String,
    answer: Option<ConflictDecision>,
}

/// Shared between a running job and the manager.
pub struct JobHandle {
    pub(super) job: Mutex<TransferJob>,
    paused: AtomicBool,
    cancelled: AtomicBool,
    started: Mutex<Option<Instant>>,
    speed: Mutex<SpeedSmoother>,
    notify: Arc<dyn Fn() + Send + Sync>,
    queue: Mutex<Option<Arc<ConflictQueue>>>,
    pending: Mutex<Option<PendingSlot>>,
}

impl JobHandle {
    pub fn new(job: TransferJob, notify: Arc<dyn Fn() + Send + Sync>) -> Self {
        Self {
            job: Mutex::new(job),
            paused: AtomicBool::new(false),
            cancelled: AtomicBool::new(false),
            started: Mutex::new(None),
            speed: Mutex::new(SpeedSmoother::default()),
            notify,
            queue: Mutex::new(None),
            pending: Mutex::new(None),
        }
    }

    pub(super) fn queue(&self) -> Option<Arc<ConflictQueue>> {
        self.queue.lock().clone()
    }

    pub(super) fn leave_queue(&self) {
        *self.queue.lock() = None;
    }

    fn answer_pending(&self, decision: ConflictDecision) {
        if let Some(slot) = self.pending.lock().as_mut() {
            slot.answer.get_or_insert(decision);
        }
    }

    /// Answers the live request; stale, repeated or foreign request ids are rejected.
    pub(super) fn resolve(
        &self,
        request_id: &str,
        decision: ConflictDecision,
    ) -> Result<(), String> {
        let mut pending = self.pending.lock();
        match pending.as_mut() {
            Some(slot) if slot.request_id == request_id && slot.answer.is_none() => {
                slot.answer = Some(decision);
                Ok(())
            }
            _ => Err("This conflict request is no longer pending".into()),
        }
    }

    /// Blocks until the user answers (or the queue already has an answer). Without a
    /// queue there is nobody to ask, so both files are kept.
    pub fn request_decision(&self, info: &ConflictInfo) -> Result<ConflictDecision, Cancelled> {
        let Some(queue) = self.queue() else {
            return Ok(ConflictDecision::KeepBoth);
        };
        if let Some(decision) = queue.preset() {
            return Ok(decision);
        }
        let request_id = new_id();
        *self.pending.lock() = Some(PendingSlot {
            request_id: request_id.clone(),
            answer: None,
        });
        self.update(|j| {
            j.pending_conflict = Some(PendingConflict {
                request_id,
                source_path: info.source_path.clone(),
                destination_path: info.destination_path.clone(),
                source_hash: info.source_hash.clone(),
                destination_hash: info.destination_hash.clone(),
            });
            j.state = JobState::AwaitingDecision;
        });
        let answer = loop {
            if self.is_cancelled() {
                break None;
            }
            let own = self.pending.lock().as_ref().and_then(|s| s.answer);
            if let Some(decision) = own.or_else(|| queue.preset()) {
                break Some(decision);
            }
            std::thread::sleep(Duration::from_millis(40));
        };
        *self.pending.lock() = None;
        let paused = self.is_paused();
        self.update(|j| {
            j.pending_conflict = None;
            if j.state == JobState::AwaitingDecision {
                j.state = if paused {
                    JobState::Paused
                } else {
                    JobState::Running
                };
            }
        });
        answer.ok_or(Cancelled)
    }

    pub fn snapshot(&self) -> TransferJob {
        let mut job = self.job.lock();
        self.refresh_speed(&mut job);
        job.clone()
    }

    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::SeqCst);
        let mut job = self.job.lock();
        if paused
            && matches!(
                job.state,
                JobState::Running | JobState::Verifying | JobState::Queued
            )
        {
            job.state = JobState::Paused;
        } else if !paused && job.state == JobState::Paused {
            job.state = if self.started.lock().is_some() {
                JobState::Running
            } else {
                JobState::Queued
            };
        }
        drop(job);
        (self.notify)();
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }

    /// Blocks while paused. Errors once the job is cancelled.
    pub fn checkpoint(&self) -> Result<(), Cancelled> {
        while self.paused.load(Ordering::SeqCst) && !self.cancelled.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(150));
        }
        if self.cancelled.load(Ordering::SeqCst) {
            Err(Cancelled)
        } else {
            Ok(())
        }
    }

    pub fn update(&self, change: impl FnOnce(&mut TransferJob)) {
        {
            let mut job = self.job.lock();
            change(&mut job);
            let mut started = self.started.lock();
            if job.state == JobState::Running && started.is_none() {
                *started = Some(Instant::now());
            }
            drop(started);
            self.refresh_speed(&mut job);
        }
        (self.notify)();
    }

    pub fn elapsed(&self) -> Option<Duration> {
        self.started.lock().map(|started| started.elapsed())
    }

    fn refresh_speed(&self, job: &mut TransferJob) {
        let speed = if job.state == JobState::Running || job.state == JobState::Verifying {
            self.speed.lock().record(Instant::now(), job.bytes_done)
        } else {
            self.speed.lock().current()
        };
        job.bytes_per_sec = speed;
        job.speed_bps = speed.unwrap_or(0);
        job.eta_secs = speed.and_then(|bps| {
            (bps > 0).then(|| job.bytes_total.saturating_sub(job.bytes_done).div_ceil(bps))
        });
    }

    pub fn add_bytes(&self, bytes: u64) {
        self.update(|j| j.bytes_done += bytes);
    }
}
