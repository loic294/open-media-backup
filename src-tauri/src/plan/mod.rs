//! Works out, per flow, which files are transferred, pending, ignored or failed,
//! and how many safe copies each source has.
mod assess;
mod catalog;
mod classify;
mod context;
mod safe_copies;
mod status;
mod vars;

pub use assess::{assess_source, FinalSet, SourceAssessment};
pub use catalog::Catalog;
pub use classify::{classify_files, classify_flow, source_files, Category, PlannedFile};
pub use context::{resolve_flow, FlowContext, PlanError};
pub use safe_copies::{safe_copy_report, DeviceCopies, FinalTarget, SafeCopyReport};
pub use status::{project_status, DestinationStatus, FlowState, FlowStatus, ProjectStatus, SourceStatus};
pub use vars::{backup_folder_name, template_vars};

use std::collections::HashMap;
use std::path::PathBuf;

/// Resolves a synced device id to its root folder on this computer.
pub trait RootResolver: Sync {
    fn device_root(&self, device_id: &str) -> Option<PathBuf>;
}

/// Last transfer errors: flow id → relative source path → message.
pub type FailureMap = HashMap<String, HashMap<String, String>>;

pub(crate) fn sanitize_segment(name: &str) -> String {
    let cleaned: String =
        name.chars().map(|c| if "/\\:*?\"<>|".contains(c) || c.is_control() { '_' } else { c }).collect();
    cleaned.trim().trim_matches('.').to_string()
}

#[cfg(test)]
mod tests;
