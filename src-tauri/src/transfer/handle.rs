use super::speed::SpeedSmoother;
use parking_lot::Mutex;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Queued,
    Running,
    Verifying,
    Paused,
    Done,
    Failed,
    Cancelled,
}

impl JobState {
    pub fn is_finished(self) -> bool {
        matches!(self, Self::Done | Self::Failed | Self::Cancelled)
    }
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
}

pub struct Cancelled;

/// Shared between a running job and the manager.
pub struct JobHandle {
    pub(super) job: Mutex<TransferJob>,
    paused: AtomicBool,
    cancelled: AtomicBool,
    started: Mutex<Option<Instant>>,
    speed: Mutex<SpeedSmoother>,
    notify: Arc<dyn Fn() + Send + Sync>,
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
        }
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
