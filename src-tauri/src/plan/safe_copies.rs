use super::Catalog;
use crate::domain::{Device, DeviceRole};
use crate::paths::TemplateVars;
use crate::rules::RuleSet;
use serde::Serialize;
use std::collections::HashMap;

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

pub struct SourceCopyFiles<'a> {
    pub folder: &'a str,
    pub files: &'a [(String, Option<String>)],
    pub vars: &'a TemplateVars,
}

/// `files` are `(path relative to the source folder, known file id)`.
pub fn safe_copy_report(
    files: &[(String, Option<String>)],
    finals: &[FinalTarget],
    catalog: &Catalog,
    rule_vars: &TemplateVars,
    temporary_copies_per_final: u32,
) -> SafeCopyReport {
    device_safe_copy_report(
        &[SourceCopyFiles {
            folder: "",
            files,
            vars: rule_vars,
        }],
        finals,
        catalog,
        temporary_copies_per_final,
    )
}

pub fn device_safe_copy_report(
    sources: &[SourceCopyFiles<'_>],
    finals: &[FinalTarget],
    catalog: &Catalog,
    temporary_copies_per_final: u32,
) -> SafeCopyReport {
    let required = |target: &FinalTarget, rel: &str, vars: &TemplateVars| {
        target.rules.is_empty() || target.rules.iter().any(|r| r.allows_with_vars(rel, vars))
    };
    let mut files: HashMap<String, Vec<(&str, &Option<String>, &TemplateVars)>> = HashMap::new();
    for source in sources {
        for (rel, id) in source.files {
            let path = if source.folder.is_empty() {
                rel.clone()
            } else {
                format!("{}/{rel}", source.folder)
            };
            files.entry(path).or_default().push((rel, id, source.vars));
        }
    }
    let copies: Vec<DeviceCopies> = finals
        .iter()
        .map(|target| {
            let wanted: Vec<_> = files
                .iter()
                .filter(|(_, views)| {
                    views
                        .iter()
                        .any(|(rel, _, vars)| required(target, rel, vars))
                })
                .collect();
            let verified = wanted
                .iter()
                .filter(|(_, views)| {
                    views.iter().all(|(_, id, _)| {
                        id.as_ref()
                            .is_some_and(|id| catalog.has_copy_on(id, &target.device.id))
                    })
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
            .filter(|(_, views)| {
                !finals
                    .iter()
                    .any(|t| views.iter().any(|(rel, _, vars)| required(t, rel, vars)))
            })
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
