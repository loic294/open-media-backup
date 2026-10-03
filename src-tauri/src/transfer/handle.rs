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
    pub errors: Vec<String>,
}

pub struct Cancelled;

/// Shared between a running job and the manager.
pub struct JobHandle {
    pub(super) job: Mutex<TransferJob>,
    paused: AtomicBool,
    cancelled: AtomicBool,
    started: Mutex<Option<Instant>>,
    notify: Arc<dyn Fn() + Send + Sync>,
}

impl JobHandle {
    pub fn new(job: TransferJob, notify: Arc<dyn Fn() + Send + Sync>) -> Self {
        Self {
            job: Mutex::new(job),
            paused: AtomicBool::new(false),
            cancelled: AtomicBool::new(false),
            started: Mutex::new(None),
            notify,
        }
    }

    pub fn snapshot(&self) -> TransferJob {
        self.job.lock().clone()
    }

    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::SeqCst);
        let mut job = self.job.lock();
        if paused && matches!(job.state, JobState::Running | JobState::Verifying | JobState::Queued) {
            job.state = JobState::Paused;
        } else if !paused && job.state == JobState::Paused {
            job.state = if self.started.lock().is_some() { JobState::Running } else { JobState::Queued };
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
            if let Some(start) = *started {
                let secs = start.elapsed().as_secs_f64();
                if secs > 0.5 {
                    job.speed_bps = (job.bytes_done as f64 / secs) as u64;
                }
            }
        }
        (self.notify)();
    }

    pub fn add_bytes(&self, bytes: u64) {
        self.update(|j| j.bytes_done += bytes);
    }
}
