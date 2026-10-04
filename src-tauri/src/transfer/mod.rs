//! Background jobs (transfers and wipes) with progress, pause and cancel.
mod check;
mod copy;
mod handle;
mod job;
mod manager;
mod speed;

pub use check::run_workspace_check;
pub use copy::{compare_files, copy_resolving, copy_verified, Comparison, CopyError, CopyOutcome};
pub use handle::{
    CheckItem, CheckOutcome, CheckResults, ConflictDecision, ConflictInfo, ConflictQueue,
    JobHandle, JobKind, JobState, PendingConflict, TransferJob,
};
pub use job::{run_transfer, run_workspace_transfer};
pub use manager::{JobSpec, TransferManager};

#[cfg(test)]
mod conflict_tests;
#[cfg(test)]
mod tests;
