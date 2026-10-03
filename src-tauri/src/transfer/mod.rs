//! Background jobs (transfers and wipes) with progress, pause and cancel.
mod copy;
mod handle;
mod job;
mod manager;

pub use copy::{copy_verified, CopyError, CopyOutcome};
pub use handle::{JobHandle, JobState, TransferJob};
pub use job::run_transfer;
pub use manager::{JobSpec, TransferManager};

#[cfg(test)]
mod tests;
