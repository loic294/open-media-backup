use super::Catalog;
use crate::domain::Device;
use crate::rules::RuleSet;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DeviceCopies {
    pub device_id: String,
    pub device_name: String,
    pub verified: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SafeCopyReport {
    pub files_total: usize,
    /// Files no final device is expected to hold (excluded by every rule set).
    pub ignored: usize,
    /// Final devices holding a verified copy of every file they are expected to hold.
    pub safe_copies: usize,
    pub copies: Vec<DeviceCopies>,
}

/// A final device and the rule sets of destinations pointing at it (empty = keep everything).
pub struct FinalTarget<'a> {
    pub device: &'a Device,
    pub rules: Vec<&'a RuleSet>,
}

/// `files` are `(path relative to the source folder, known file id)`.
pub fn safe_copy_report(
    files: &[(String, Option<String>)],
    finals: &[FinalTarget],
    catalog: &Catalog,
) -> SafeCopyReport {
    let required = |target: &FinalTarget, rel: &str| {
        target.rules.is_empty() || target.rules.iter().any(|r| r.allows(rel))
    };
    let copies: Vec<DeviceCopies> = finals
        .iter()
        .map(|target| {
            let wanted: Vec<_> = files
                .iter()
                .filter(|(rel, _)| required(target, rel))
                .collect();
            let verified = wanted
                .iter()
                .filter(|(_, id)| {
                    id.as_ref()
                        .is_some_and(|id| catalog.has_copy_on(id, &target.device.id))
                })
                .count();
            DeviceCopies {
                device_id: target.device.id.clone(),
                device_name: target.device.name.clone(),
                verified,
                total: wanted.len(),
            }
        })
        .collect();
    let ignored = if finals.is_empty() {
        0
    } else {
        files
            .iter()
            .filter(|(rel, _)| !finals.iter().any(|t| required(t, rel)))
            .count()
    };
    SafeCopyReport {
        files_total: files.len(),
        ignored,
        safe_copies: copies
            .iter()
            .filter(|c| c.total > 0 && c.verified == c.total)
            .count(),
        copies,
    }
}
