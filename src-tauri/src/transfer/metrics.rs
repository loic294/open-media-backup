//! Computer-local measurements, independent of budgeted UI progress and learned speeds.
use super::{JobKind, JobState};
use crate::plan::FlowContext;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalysisContext {
    pub pair_id: String,
    pub space_id: String,
    pub space_name: String,
    pub source_id: String,
    pub destination_id: String,
    pub source_device_id: String,
    pub destination_device_id: String,
    pub source_name: String,
    pub destination_name: String,
    pub source_device_name: String,
    pub destination_device_name: String,
    pub hash_algo: String,
    pub verify_mode: String,
}

impl AnalysisContext {
    pub fn from_flow(ctx: &FlowContext) -> Self {
        Self {
            pair_id: serde_json::to_string(&(
                &ctx.space.id,
                &ctx.source.id,
                &ctx.destination.id,
                &ctx.source_device.id,
                &ctx.dest_device.id,
            ))
            .expect("string tuple serialization"),
            space_id: ctx.space.id.clone(),
            space_name: ctx.space.name.clone(),
            source_id: ctx.source.id.clone(),
            destination_id: ctx.destination.id.clone(),
            source_device_id: ctx.source_device.id.clone(),
            destination_device_id: ctx.dest_device.id.clone(),
            source_name: ctx.source.resolved_task_name(&ctx.source_device).into(),
            destination_name: ctx.destination.resolved_task_name(&ctx.dest_device).into(),
            source_device_name: ctx.source_device.name.clone(),
            destination_device_name: ctx.dest_device.name.clone(),
            hash_algo: match ctx.space.hash_algo {
                crate::domain::HashAlgo::Blake3 => "blake3",
                crate::domain::HashAlgo::Xxh64 => "xxh64",
            }
            .into(),
            verify_mode: match ctx.space.verify_mode {
                crate::domain::VerifyMode::Inline => "inline",
                crate::domain::VerifyMode::Reread => "reread",
            }
            .into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisPhase {
    Queued,
    Other,
    Copy,
    SourceCheck,
    DestinationCheck,
    RemoteCheck,
    Paused,
    AwaitingDecision,
    Finished,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AnalysisMetrics {
    pub queued_secs: f64,
    pub other_secs: f64,
    pub copy_secs: f64,
    pub source_check_secs: f64,
    pub destination_check_secs: f64,
    pub remote_check_secs: f64,
    pub paused_secs: f64,
    pub decision_secs: f64,
    pub copy_bytes: u64,
    pub committed_bytes: u64,
    pub source_check_bytes: u64,
    pub destination_check_bytes: u64,
    pub remote_check_bytes: u64,
    pub transferred_files: u64,
    pub adopted_files: u64,
    pub skipped_files: u64,
}

impl AnalysisMetrics {
    pub fn active_secs(&self) -> f64 {
        self.other_secs
            + self.copy_secs
            + self.source_check_secs
            + self.destination_check_secs
            + self.remote_check_secs
    }

    pub(crate) fn add(&mut self, other: &Self) {
        self.queued_secs += other.queued_secs;
        self.other_secs += other.other_secs;
        self.copy_secs += other.copy_secs;
        self.source_check_secs += other.source_check_secs;
        self.destination_check_secs += other.destination_check_secs;
        self.remote_check_secs += other.remote_check_secs;
        self.paused_secs += other.paused_secs;
        self.decision_secs += other.decision_secs;
        self.copy_bytes += other.copy_bytes;
        self.committed_bytes += other.committed_bytes;
        self.source_check_bytes += other.source_check_bytes;
        self.destination_check_bytes += other.destination_check_bytes;
        self.remote_check_bytes += other.remote_check_bytes;
        self.transferred_files += other.transferred_files;
        self.adopted_files += other.adopted_files;
        self.skipped_files += other.skipped_files;
    }

    fn accrue(&mut self, phase: AnalysisPhase, secs: f64) {
        match phase {
            AnalysisPhase::Queued => self.queued_secs += secs,
            AnalysisPhase::Other => self.other_secs += secs,
            AnalysisPhase::Copy => self.copy_secs += secs,
            AnalysisPhase::SourceCheck => self.source_check_secs += secs,
            AnalysisPhase::DestinationCheck => self.destination_check_secs += secs,
            AnalysisPhase::RemoteCheck => self.remote_check_secs += secs,
            AnalysisPhase::Paused => self.paused_secs += secs,
            AnalysisPhase::AwaitingDecision => self.decision_secs += secs,
            AnalysisPhase::Finished => {}
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisKind {
    Transfer,
    Check,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisState {
    Queued,
    Running,
    Verifying,
    AwaitingDecision,
    Paused,
    Done,
    Failed,
    Cancelled,
    Interrupted,
}

impl AnalysisState {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Done | Self::Failed | Self::Cancelled | Self::Interrupted
        )
    }
}

impl From<JobState> for AnalysisState {
    fn from(state: JobState) -> Self {
        match state {
            JobState::Queued => Self::Queued,
            JobState::Running => Self::Running,
            JobState::Verifying => Self::Verifying,
            JobState::AwaitingDecision => Self::AwaitingDecision,
            JobState::Paused => Self::Paused,
            JobState::Done => Self::Done,
            JobState::Failed => Self::Failed,
            JobState::Cancelled => Self::Cancelled,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnalysisJob {
    pub id: String,
    pub context: AnalysisContext,
    pub kind: AnalysisKind,
    pub state: AnalysisState,
    pub phase: AnalysisPhase,
    pub created_at: i64,
    pub updated_at: i64,
    pub finished_at: Option<i64>,
    pub metrics: AnalysisMetrics,
    pub error_count: u64,
}

pub(crate) trait AnalysisClock: Send + Sync {
    fn monotonic(&self) -> Duration;
    fn epoch_ms(&self) -> i64;
}

pub(crate) struct SystemClock(Instant);

impl Default for SystemClock {
    fn default() -> Self {
        Self(Instant::now())
    }
}

impl AnalysisClock for SystemClock {
    fn monotonic(&self) -> Duration {
        self.0.elapsed()
    }
    fn epoch_ms(&self) -> i64 {
        chrono::Utc::now().timestamp_millis()
    }
}

pub(crate) struct PhaseTracker {
    pub job: AnalysisJob,
    operation: AnalysisPhase,
    clock: Arc<dyn AnalysisClock>,
    last: Duration,
    persisted: Duration,
    persisted_state: Option<AnalysisState>,
}

impl PhaseTracker {
    pub fn new(id: String, context: AnalysisContext, kind: JobKind) -> Self {
        Self::with_clock(id, context, kind, Arc::new(SystemClock::default()))
    }

    pub(crate) fn with_clock(
        id: String,
        context: AnalysisContext,
        kind: JobKind,
        clock: Arc<dyn AnalysisClock>,
    ) -> Self {
        let now = clock.monotonic();
        let timestamp = clock.epoch_ms();
        Self {
            job: AnalysisJob {
                id,
                context,
                kind: match kind {
                    JobKind::Check => AnalysisKind::Check,
                    JobKind::Transfer => AnalysisKind::Transfer,
                    JobKind::Wipe => panic!("wipe jobs have no analysis"),
                },
                state: AnalysisState::Queued,
                phase: AnalysisPhase::Queued,
                created_at: timestamp,
                updated_at: timestamp,
                finished_at: None,
                metrics: AnalysisMetrics::default(),
                error_count: 0,
            },
            operation: AnalysisPhase::Other,
            clock,
            last: now,
            persisted: now,
            persisted_state: None,
        }
    }

    pub fn refresh(&mut self, state: JobState, errors: u64) {
        if self.job.state.is_terminal() {
            return;
        }
        let now = self.clock.monotonic();
        self.job
            .metrics
            .accrue(self.job.phase, now.saturating_sub(self.last).as_secs_f64());
        self.last = now;
        self.job.updated_at = self.clock.epoch_ms().max(self.job.updated_at);
        self.job.error_count = errors;
        self.job.state = state.into();
        self.job.phase = match state {
            JobState::Queued => AnalysisPhase::Queued,
            JobState::Paused => AnalysisPhase::Paused,
            JobState::AwaitingDecision => AnalysisPhase::AwaitingDecision,
            JobState::Done | JobState::Failed | JobState::Cancelled => {
                self.job.finished_at = Some(self.job.updated_at);
                AnalysisPhase::Finished
            }
            _ => self.operation,
        };
    }

    pub fn set_operation(
        &mut self,
        phase: AnalysisPhase,
        state: JobState,
        errors: u64,
    ) -> AnalysisPhase {
        self.refresh(state, errors);
        let previous = self.operation;
        self.operation = phase;
        self.refresh(state, errors);
        previous
    }

    pub fn metrics(&mut self) -> Option<&mut AnalysisMetrics> {
        (!self.job.state.is_terminal()).then_some(&mut self.job.metrics)
    }

    pub fn persistence_due(&mut self) -> bool {
        let now = self.clock.monotonic();
        if self.persisted_state != Some(self.job.state)
            || (!self.job.state.is_terminal()
                && now.saturating_sub(self.persisted) >= Duration::from_secs(5))
        {
            self.persisted_state = Some(self.job.state);
            self.persisted = now;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[derive(Default)]
    pub(crate) struct TestClock(AtomicU64);
    impl TestClock {
        pub(crate) fn advance(&self, secs: u64) {
            self.0.fetch_add(secs, Ordering::SeqCst);
        }
    }
    impl AnalysisClock for TestClock {
        fn monotonic(&self) -> Duration {
            Duration::from_secs(self.0.load(Ordering::SeqCst))
        }
        fn epoch_ms(&self) -> i64 {
            1000 + self.monotonic().as_millis() as i64
        }
    }

    pub(crate) fn context() -> AnalysisContext {
        AnalysisContext {
            pair_id: r#"["s","a","b","c","d"]"#.into(),
            space_id: "s".into(),
            space_name: "Space".into(),
            source_id: "a".into(),
            destination_id: "b".into(),
            source_device_id: "c".into(),
            destination_device_id: "d".into(),
            source_name: "Source".into(),
            destination_name: "Destination".into(),
            source_device_name: "Card".into(),
            destination_device_name: "Disk".into(),
            hash_algo: "blake3".into(),
            verify_mode: "reread".into(),
        }
    }

    #[test]
    fn exclusive_phases_restore_operations_and_freeze_terminal() {
        let clock = Arc::new(TestClock::default());
        let mut t =
            PhaseTracker::with_clock("j".into(), context(), JobKind::Transfer, clock.clone());
        assert!(t.persistence_due());
        clock.advance(2);
        t.refresh(JobState::Running, 0);
        clock.advance(3);
        t.set_operation(AnalysisPhase::Copy, JobState::Running, 0);
        assert!(t.persistence_due());
        clock.advance(4);
        t.refresh(JobState::Paused, 0);
        clock.advance(10);
        t.refresh(JobState::Running, 0);
        assert_eq!(t.job.phase, AnalysisPhase::Copy);
        clock.advance(1);
        t.refresh(JobState::AwaitingDecision, 0);
        clock.advance(20);
        t.refresh(JobState::Running, 0);
        assert_eq!(t.job.phase, AnalysisPhase::Copy);
        clock.advance(2);
        t.set_operation(AnalysisPhase::SourceCheck, JobState::Running, 0);
        clock.advance(6);
        t.set_operation(AnalysisPhase::DestinationCheck, JobState::Verifying, 0);
        clock.advance(7);
        t.refresh(JobState::Done, 2);
        let frozen = t.job.clone();
        clock.advance(100);
        t.refresh(JobState::Running, 0);
        assert_eq!(t.job, frozen);
        assert_eq!(t.job.metrics.queued_secs, 2.0);
        assert_eq!(t.job.metrics.other_secs, 3.0);
        assert_eq!(t.job.metrics.copy_secs, 7.0);
        assert_eq!(t.job.metrics.source_check_secs, 6.0);
        assert_eq!(t.job.metrics.destination_check_secs, 7.0);
        assert_eq!(t.job.metrics.paused_secs, 10.0);
        assert_eq!(t.job.metrics.decision_secs, 20.0);
        assert_eq!(t.job.metrics.active_secs(), 23.0);
        assert!(t.metrics().is_none());
    }

    #[test]
    fn checkpoints_are_bounded_and_state_changes_immediate() {
        let clock = Arc::new(TestClock::default());
        let mut t = PhaseTracker::with_clock("j".into(), context(), JobKind::Check, clock.clone());
        assert!(t.persistence_due());
        t.refresh(JobState::Running, 0);
        assert!(t.persistence_due());
        for _ in 0..4 {
            clock.advance(1);
            t.refresh(JobState::Running, 0);
            assert!(!t.persistence_due());
        }
        clock.advance(1);
        t.refresh(JobState::Running, 0);
        assert!(t.persistence_due());
        t.refresh(JobState::Cancelled, 0);
        assert!(t.persistence_due());
        clock.advance(10);
        assert!(!t.persistence_due());
    }

    #[test]
    fn queued_pause_and_partial_failure_or_cancellation_are_frozen() {
        for terminal in [JobState::Failed, JobState::Cancelled] {
            let clock = Arc::new(TestClock::default());
            let mut t =
                PhaseTracker::with_clock("j".into(), context(), JobKind::Transfer, clock.clone());
            clock.advance(2);
            t.refresh(JobState::Paused, 0);
            clock.advance(3);
            t.refresh(JobState::Queued, 0);
            assert_eq!(t.job.phase, AnalysisPhase::Queued);
            clock.advance(1);
            t.refresh(JobState::Running, 0);
            t.set_operation(AnalysisPhase::Copy, JobState::Running, 0);
            t.metrics().unwrap().copy_bytes = 10;
            clock.advance(5);
            t.refresh(terminal, 2);
            let frozen = t.job.clone();
            assert_eq!(frozen.metrics.queued_secs, 3.0);
            assert_eq!(frozen.metrics.paused_secs, 3.0);
            assert_eq!(frozen.metrics.copy_secs, 5.0);
            assert_eq!(frozen.error_count, 2);
            assert_eq!(frozen.phase, AnalysisPhase::Finished);
            clock.advance(100);
            t.set_operation(AnalysisPhase::Other, JobState::Running, 0);
            assert_eq!(t.job, frozen);
        }
    }
}
