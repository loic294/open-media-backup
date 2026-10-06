use super::Catalog;
use crate::domain::{Device, DeviceRole};
use crate::paths::TemplateVars;
use crate::rules::RuleSet;
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SafeCopyState {
    Safe,
    Unsafe,
    Excluded,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SafeCopyFile {
    pub path: String,
    pub state: SafeCopyState,
    pub safe_copies: usize,
    #[serde(skip)]
    pub(crate) final_copies: usize,
    pub verified_destinations: Vec<String>,
    pub acknowledged_destinations: Vec<String>,
    pub reasons: Vec<String>,
}

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
    /// Files excluded by every overlapping source view's safe-copy rules.
    pub ignored: usize,
    /// Effective safe copies: every final copy counts as one; temporary copies count in groups.
    pub safe_copies: usize,
    pub final_safe_copies: usize,
    pub temporary_safe_copies: usize,
    pub copies: Vec<DeviceCopies>,
    #[serde(skip)]
    pub files: Vec<SafeCopyFile>,
}

impl SafeCopyReport {
    pub fn classify_files(&mut self, required_copies: Option<u32>, source_role: DeviceRole) {
        for file in &mut self.files {
            if file.state == SafeCopyState::Excluded {
                continue;
            }
            let threshold_met =
                required_copies.is_some_and(|required| file.safe_copies as u32 >= required);
            let identity_known = !file
                .reasons
                .iter()
                .any(|reason| reason.starts_with("File identity is not verified"));
            let has_coverage = !file
                .reasons
                .iter()
                .any(|reason| reason.starts_with("No eligible"));
            let needs_final = source_role == DeviceRole::Temporary && file.final_copies == 0;
            file.state = if threshold_met && identity_known && has_coverage && !needs_final {
                SafeCopyState::Safe
            } else {
                SafeCopyState::Unsafe
            };
            if !threshold_met {
                file.reasons.push(match required_copies {
                    Some(required) => format!(
                        "Needs {} safe copies; currently {}",
                        required, file.safe_copies
                    ),
                    None => "Create or select a project to set safe-copy requirements".into(),
                });
            }
            if needs_final {
                file.reasons.push(
                    "Needs a final destination copy before this temporary device can be wiped"
                        .into(),
                );
            }
        }
    }
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
    pub safe_copy_rules: Option<&'a RuleSet>,
}

type SourceCopyView<'a> = (
    &'a str,
    &'a Option<String>,
    &'a TemplateVars,
    Option<String>,
);

/// `files` are `(path relative to the source folder, known file id)`.
pub fn safe_copy_report(
    files: &[(String, Option<String>)],
    finals: &[FinalTarget],
    catalog: &Catalog,
    rule_vars: &TemplateVars,
    temporary_copies_per_final: u32,
    skip_counts_as_safe_copy: bool,
    space_id: &str,
) -> SafeCopyReport {
    device_safe_copy_report(
        &[SourceCopyFiles {
            folder: "",
            files,
            vars: rule_vars,
            safe_copy_rules: None,
        }],
        finals,
        catalog,
        temporary_copies_per_final,
        skip_counts_as_safe_copy,
        space_id,
    )
}

pub fn device_safe_copy_report(
    sources: &[SourceCopyFiles<'_>],
    finals: &[FinalTarget],
    catalog: &Catalog,
    temporary_copies_per_final: u32,
    skip_counts_as_safe_copy: bool,
    space_id: &str,
) -> SafeCopyReport {
    let required = |target: &FinalTarget, rel: &str, vars: &TemplateVars| {
        target.rules.is_empty() || target.rules.iter().any(|r| r.allows_with_vars(rel, vars))
    };
    let mut files: BTreeMap<String, Vec<SourceCopyView<'_>>> = BTreeMap::new();
    for source in sources {
        for (rel, id) in source.files {
            let path = if source.folder.is_empty() {
                rel.clone()
            } else {
                format!("{}/{rel}", source.folder)
            };
            let exclusion = source
                .safe_copy_rules
                .and_then(|rules| rules.ignore_reason(rel, source.vars))
                .map(|reason| reason.replace("destination rule", "source safe-copy rule"));
            files
                .entry(path)
                .or_default()
                .push((rel, id, source.vars, exclusion));
        }
    }
    let included = |views: &Vec<SourceCopyView<'_>>| {
        views.iter().any(|(_, _, _, exclusion)| exclusion.is_none())
    };
    let covered = |target: &FinalTarget, views: &Vec<SourceCopyView<'_>>| {
        views
            .iter()
            .any(|(rel, _, vars, exclusion)| exclusion.is_none() && required(target, rel, vars))
    };
    let verified = |target: &FinalTarget, views: &Vec<SourceCopyView<'_>>| {
        views
            .iter()
            .filter(|(_, _, _, exclusion)| exclusion.is_none())
            .all(|(_, id, _, _)| {
                id.as_ref()
                    .is_some_and(|id| catalog.has_copy_on(id, &target.device.id))
            })
    };
    let acknowledged = |target: &FinalTarget, views: &Vec<SourceCopyView<'_>>| {
        views
            .iter()
            .filter(|(_, _, _, exclusion)| exclusion.is_none())
            .all(|(_, id, _, _)| {
                id.as_ref().is_some_and(|id| {
                    catalog.has_copy_on(id, &target.device.id)
                        || catalog.has_safe_copy_override(id, &target.device.id, space_id)
                })
            })
    };
    let copies: Vec<DeviceCopies> = finals
        .iter()
        .map(|target| {
            let wanted: Vec<_> = files
                .iter()
                .filter(|(_, views)| {
                    views.iter().any(|(rel, _, vars, exclusion)| {
                        exclusion.is_none() && required(target, rel, vars)
                    })
                })
                .collect();
            let verified = wanted
                .iter()
                .filter(|(_, views)| {
                    verified(target, views)
                        || (skip_counts_as_safe_copy && acknowledged(target, views))
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
    let ignored = files.values().filter(|views| !included(views)).count();
    let required_files = files.len() - ignored;
    let fully_verified = |role: DeviceRole| {
        finals
            .iter()
            .zip(copies.iter())
            .filter(|(target, copy)| {
                target.device.role == role
                    && copy.total > 0
                    && copy.total == required_files
                    && copy.verified == copy.total
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
    let details = files
        .iter()
        .map(|(path, views)| {
            if !included(views) {
                let mut reasons: Vec<_> = views.iter().filter_map(|(_, _, _, reason)| reason.clone()).collect();
                reasons.sort();
                reasons.dedup();
                return SafeCopyFile {
                    path: path.clone(),
                    state: SafeCopyState::Excluded,
                    safe_copies: 0,
                    final_copies: 0,
                    verified_destinations: vec![],
                    acknowledged_destinations: vec![],
                    reasons,
                };
            }
            let mut verified_destinations = Vec::new();
            let mut acknowledged_destinations = Vec::new();
            let mut reasons = Vec::new();
            let mut final_count = 0;
            let mut temporary_count = 0;
            let mut coverage = false;
            for target in finals {
                if !covered(target, views) {
                    continue;
                }
                coverage = true;
                let bytes_verified = verified(target, views);
                let ack = !bytes_verified && acknowledged(target, views);
                if bytes_verified {
                    verified_destinations.push(target.device.name.clone());
                } else if ack {
                    acknowledged_destinations.push(target.device.name.clone());
                    reasons.push(format!(
                        "{}: skipped-file acknowledgment, not byte-verified{}",
                        target.device.name,
                        if skip_counts_as_safe_copy { "" } else { "; does not count under this space's policy" }
                    ));
                } else {
                    reasons.push(format!("{}: missing a verified copy; transfer or check this destination", target.device.name));
                }
                if bytes_verified || (skip_counts_as_safe_copy && ack) {
                    match target.device.role {
                        DeviceRole::Final => final_count += 1,
                        DeviceRole::Temporary => temporary_count += 1,
                        DeviceRole::Original => {}
                    }
                }
            }
            if !coverage {
                reasons.push("No eligible safe-copy destination rule covers this file; change destination rules or add a destination".into());
            }
            if views.iter().any(|(_, id, _, exclusion)| exclusion.is_none() && id.is_none()) {
                reasons.push("File identity is not verified; transfer or check this file before wiping".into());
            }
            verified_destinations.sort();
            acknowledged_destinations.sort();
            let safe_copies = final_count + if temporary_copies_per_final > 0 {
                temporary_count / temporary_copies_per_final as usize
            } else { 0 };
            SafeCopyFile {
                path: path.clone(),
                // This report has no project threshold; classify_files applies the source policy.
                state: SafeCopyState::Unsafe,
                safe_copies,
                final_copies: final_count,
                verified_destinations,
                acknowledged_destinations,
                reasons,
            }
        })
        .collect();
    SafeCopyReport {
        files_total: files.len(),
        ignored,
        safe_copies: final_safe_copies + temporary_equivalent,
        final_safe_copies,
        temporary_safe_copies,
        copies,
        files: details,
    }
}
