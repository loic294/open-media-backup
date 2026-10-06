use super::history::HistoryRecorder;
use super::metrics::{AnalysisContext, AnalysisMetrics, AnalysisPhase, PhaseTracker};
use super::power::PowerController;
use super::speed::SpeedSmoother;
use super::AnalysisJob;
use crate::domain::new_id;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
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
    Verified,
    Untracked,
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
    pub verified: usize,
    pub untracked: usize,
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
    pub warnings: Vec<String>,
    pub remote_hash_active: bool,
    pub kind: JobKind,
    pub pending_conflict: Option<PendingConflict>,
    pub check_results: Option<CheckResults>,
    pub analysis: Option<AnalysisJob>,
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
            warnings: vec![],
            remote_hash_active: false,
            kind,
            pending_conflict: None,
            check_results: None,
            analysis: None,
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

struct JobPower {
    controller: PowerController,
    id: String,
    active: bool,
}

impl Drop for JobPower {
    fn drop(&mut self) {
        if self.active {
            self.controller.set_active(&self.id, false);
        }
    }
}

/// Shared between a running job and the manager.
pub struct JobHandle {
    pub(super) job: Mutex<TransferJob>,
    paused: AtomicBool,
    cancelled: AtomicBool,
    worker_started: AtomicBool,
    started: Mutex<Option<Instant>>,
    speed: Mutex<SpeedSmoother>,
    notify: Arc<dyn Fn() + Send + Sync>,
    queue: Mutex<Option<Arc<ConflictQueue>>>,
    pending: Mutex<Option<PendingSlot>>,
    power: Option<Mutex<JobPower>>,
    analysis: Mutex<Option<PhaseTracker>>,
    analysis_errors: AtomicU64,
    history: Option<HistoryRecorder>,
    pub(super) final_recorded: AtomicBool,
}

impl JobHandle {
    pub fn new(job: TransferJob, notify: Arc<dyn Fn() + Send + Sync>) -> Self {
        Self {
            job: Mutex::new(job),
            paused: AtomicBool::new(false),
            cancelled: AtomicBool::new(false),
            worker_started: AtomicBool::new(false),
            started: Mutex::new(None),
            speed: Mutex::new(SpeedSmoother::default()),
            notify,
            queue: Mutex::new(None),
            pending: Mutex::new(None),
            power: None,
            analysis: Mutex::new(None),
            analysis_errors: AtomicU64::new(0),
            history: None,
            final_recorded: AtomicBool::new(true),
        }
    }

    pub(super) fn with_analysis(
        mut self,
        context: Option<AnalysisContext>,
        history: Option<HistoryRecorder>,
    ) -> Self {
        if let Some(context) = context {
            let mut job = self.job.lock();
            if job.kind != JobKind::Wipe {
                let tracker = PhaseTracker::new(job.id.clone(), context, job.kind);
                job.analysis = Some(tracker.job.clone());
                *self.analysis.lock() = Some(tracker);
                self.final_recorded.store(false, Ordering::SeqCst);
            }
        }
        self.history = history;
        self
    }

    pub(super) fn record_enqueue(&self) {
        let receiver = {
            let mut job = self.job.lock();
            self.refresh_analysis(&mut job);
            match (&self.history, &job.analysis) {
                (Some(history), Some(analysis)) => history.record(analysis.clone(), true),
                _ => None,
            }
        };
        if let Some(receiver) = receiver {
            if let Err(error) = receiver.recv() {
                log::error!("Speed analysis enqueue acknowledgement failed: {error}");
            }
        }
    }

    pub(super) fn with_power(
        job: TransferJob,
        notify: Arc<dyn Fn() + Send + Sync>,
        controller: PowerController,
    ) -> Self {
        let power = (job.kind == JobKind::Transfer).then(|| {
            Mutex::new(JobPower {
                controller,
                id: job.id.clone(),
                active: false,
            })
        });
        Self {
            power,
            ..Self::new(job, notify)
        }
    }

    pub(super) fn start_work(&self) {
        self.update(|job| {
            self.worker_started.store(true, Ordering::SeqCst);
            job.state = if self.is_paused() {
                JobState::Paused
            } else {
                JobState::Running
            };
        });
    }

    fn update_power(&self, job: &TransferJob) {
        if let Some(power) = &self.power {
            let mut power = power.lock();
            let active = self.worker_started.load(Ordering::SeqCst)
                && !self.is_paused()
                && matches!(job.state, JobState::Running | JobState::Verifying);
            if power.active != active {
                power.controller.set_active(&power.id, active);
                power.active = active;
            }
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
        self.update(|j| {
            j.pending_conflict = None;
            if j.state == JobState::AwaitingDecision && !self.is_cancelled() {
                j.state = if self.is_paused() {
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
        self.refresh_analysis(&mut job);
        self.record_if_due(&job);
        job.clone()
    }

    pub(super) fn telemetry_snapshot(&self) -> TransferJob {
        let mut job = self.job.lock();
        self.refresh_analysis(&mut job);
        self.record_if_due(&job);
        job.clone()
    }

    pub fn set_paused(&self, paused: bool) {
        self.update(|job| {
            self.paused.store(paused, Ordering::SeqCst);
            if paused
                && matches!(
                    job.state,
                    JobState::Running | JobState::Verifying | JobState::Queued
                )
            {
                job.state = JobState::Paused;
            } else if !paused && job.state == JobState::Paused {
                job.state = if self.worker_started.load(Ordering::SeqCst)
                    || self.started.lock().is_some()
                {
                    JobState::Running
                } else {
                    JobState::Queued
                };
            }
        });
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
        let acknowledgement = {
            let mut job = self.job.lock();
            self.refresh_analysis(&mut job);
            change(&mut job);
            if self.is_paused() && matches!(job.state, JobState::Running | JobState::Verifying) {
                job.state = JobState::Paused;
            }
            let mut started = self.started.lock();
            if job.state == JobState::Running && started.is_none() {
                *started = Some(Instant::now());
            }
            drop(started);
            self.refresh_speed(&mut job);
            self.update_power(&job);
            self.refresh_analysis(&mut job);
            self.record_if_due(&job)
        };
        if let Some(receiver) = acknowledgement {
            if let Err(error) = receiver.recv() {
                log::error!("Speed analysis terminal acknowledgement failed: {error}");
            }
            self.final_recorded.store(true, Ordering::SeqCst);
        } else if self.history.is_none() {
            self.final_recorded.store(true, Ordering::SeqCst);
        }
        (self.notify)();
    }

    fn refresh_analysis(&self, job: &mut TransferJob) {
        if let Some(tracker) = self.analysis.lock().as_mut() {
            let errors = job.errors.len() as u64
                + self.analysis_errors.load(Ordering::SeqCst).max(
                    job.check_results
                        .as_ref()
                        .map_or(0, |results| results.errors as u64),
                );
            tracker.refresh(job.state, errors);
            job.analysis = Some(tracker.job.clone());
        }
    }

    pub(crate) fn analysis_error(&self) {
        self.analysis_errors.fetch_add(1, Ordering::SeqCst);
    }

    #[cfg(test)]
    pub(super) fn with_analysis_clock(self, clock: Arc<dyn super::metrics::AnalysisClock>) -> Self {
        {
            let mut job = self.job.lock();
            if let Some(analysis) = &job.analysis {
                let tracker = PhaseTracker::with_clock(
                    job.id.clone(),
                    analysis.context.clone(),
                    job.kind,
                    clock,
                );
                job.analysis = Some(tracker.job.clone());
                *self.analysis.lock() = Some(tracker);
            }
        }
        self
    }

    fn record_if_due(&self, job: &TransferJob) -> Option<std::sync::mpsc::Receiver<()>> {
        let mut analysis = self.analysis.lock();
        let tracker = analysis.as_mut()?;
        if !tracker.persistence_due() {
            return None;
        }
        self.history
            .as_ref()
            .and_then(|history| history.record(tracker.job.clone(), job.state.is_finished()))
    }

    pub(crate) fn analysis_metrics(&self, change: impl FnOnce(&mut AnalysisMetrics)) {
        let mut job = self.job.lock();
        self.refresh_analysis(&mut job);
        if let Some(metrics) = self
            .analysis
            .lock()
            .as_mut()
            .and_then(PhaseTracker::metrics)
        {
            change(metrics);
        }
        self.refresh_analysis(&mut job);
        // The emitter persists at most every five seconds; chunks never touch SQLite.
    }

    pub(crate) fn analysis_phase(&self, phase: AnalysisPhase) -> PhaseGuard<'_> {
        PhaseGuard {
            handle: self,
            previous: self.set_analysis_phase(phase),
        }
    }

    fn set_analysis_phase(&self, phase: AnalysisPhase) -> Option<AnalysisPhase> {
        let mut job = self.job.lock();
        let errors = job
            .analysis
            .as_ref()
            .map_or(0, |analysis| analysis.error_count);
        let previous = self
            .analysis
            .lock()
            .as_mut()
            .map(|tracker| tracker.set_operation(phase, job.state, errors));
        self.refresh_analysis(&mut job);
        previous
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

pub(crate) struct PhaseGuard<'a> {
    handle: &'a JobHandle,
    previous: Option<AnalysisPhase>,
}

impl Drop for PhaseGuard<'_> {
    fn drop(&mut self) {
        if let Some(previous) = self.previous {
            self.handle.set_analysis_phase(previous);
        }
    }
}
