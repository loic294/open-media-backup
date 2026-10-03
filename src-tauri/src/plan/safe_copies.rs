use super::Catalog;
use crate::domain::{Device, DeviceRole};
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
    /// Files no safe-copy destination is expected to hold (excluded by every rule set).
    pub ignored: usize,
    /// Effective safe copies: every final copy counts as one; temporary copies count in groups.
    pub safe_copies: usize,
    pub final_safe_copies: usize,
    pub temporary_safe_copies: usize,
    pub copies: Vec<DeviceCopies>,
}

/// A destination device and the rule sets of destinations pointing at it (empty = keep everything).
pub struct FinalTarget<'a> {
    pub device: &'a Device,
    pub rules: Vec<&'a RuleSet>,
}

/// `files` are `(path relative to the source folder, known file id)`.
pub fn safe_copy_report(
    files: &[(String, Option<String>)],
    finals: &[FinalTarget],
    catalog: &Catalog,
    temporary_copies_per_final: u32,
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
    let fully_verified = |role: DeviceRole| {
        finals
            .iter()
            .zip(copies.iter())
            .filter(|(target, copy)| {
                target.device.role == role && copy.total > 0 && copy.verified == copy.total
            })
            .count()
    };
    let final_safe_copies = fully_verified(DeviceRole::Final);
    let temporary_safe_copies = fully_verified(DeviceRole::Temporary);
    let temporary_equivalent = if temporary_copies_per_final > 0 {
        temporary_safe_copies / temporary_copies_per_final as usize
    } else {
        0
    };
    SafeCopyReport {
        files_total: files.len(),
        ignored,
        safe_copies: final_safe_copies + temporary_equivalent,
        final_safe_copies,
        temporary_safe_copies,
        copies,
    }
}
