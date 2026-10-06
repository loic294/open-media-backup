//! Background jobs (transfers and wipes) with progress, pause and cancel.
mod check;
mod copy;
pub mod destination_hasher;
mod handle;
mod history;
mod job;
mod manager;
pub mod metrics;
mod power;
mod speed;

pub use check::run_workspace_check;
pub use copy::{compare_files, copy_resolving, copy_verified, Comparison, CopyError, CopyOutcome};
pub use handle::{
    CheckItem, CheckOutcome, CheckResults, ConflictDecision, ConflictInfo, ConflictQueue,
    JobHandle, JobKind, JobState, PendingConflict, TransferJob,
};
pub use job::{run_transfer, run_workspace_transfer};
pub use manager::{JobSpec, ResourceClaim, TransferManager};
pub use metrics::{AnalysisContext, AnalysisJob, AnalysisMetrics, AnalysisPhase};
pub(crate) use power::PowerController;

#[cfg(test)]
mod analysis_tests;
#[cfg(test)]
mod conflict_tests;
#[cfg(test)]
mod power_tests;
#[cfg(test)]
mod remote_tests;
#[cfg(test)]
mod tests;
