use super::safe_copies::{device_safe_copy_report, SourceCopyFiles};
use super::*;
use crate::domain::{
    Destination, DestinationKind, Device, DeviceRole, FileCopy, FileRecord, FileRule, HashAlgo,
    RuleAction, RuleExpr, RuleSyntax, Source, Space,
};
use crate::paths::TemplateVars;
use crate::rules::RuleSet;
use crate::testing::Fixture;

fn copy(fx: &Fixture, hash: &str, device: &str, path: &str) {
    let id = FileRecord::id_for(HashAlgo::Xxh64, hash);
    fx.store
        .put(&FileRecord {
            id: id.clone(),
            hash: hash.into(),
            size: 1,
            ..Default::default()
        })
        .unwrap();
    fx.store
        .put(&FileCopy {
            id: FileCopy::id_for(&id, device, path),
            file_id: id,
            device_id: device.into(),
            path: path.into(),
            ..Default::default()
        })
        .unwrap();
}

fn sibling(fx: &Fixture, path: &str) -> Source {
    let source = Source {
        id: "sibling".into(),
        path_template: path.into(),
        position: 1,
        ..fx.source.clone()
    };
    fx.store.put(&source).unwrap();
    source
}

fn status(fx: &Fixture) -> ProjectStatus {
    project_status(
        &fx.store,
        &fx.resolver,
        &Catalog::load(&fx.store).unwrap(),
        &fx.project.id,
        &FailureMap::new(),
    )
    .unwrap()
}

#[test]
fn source_exclusion_is_relative_and_overlapping_required_view_wins() {
    let fx = Fixture::new();
    let mut source = fx.source.clone();
    source.safe_copy_rules = vec![FileRule::path(
        RuleAction::Exclude,
        RuleSyntax::Glob,
        "A.JPG",
    )];
    fx.store.put(&source).unwrap();
    fx.write_card_file("DCIM/A.JPG", b"a");
    let finals = FinalSet::load_for_space(&fx.store, "space").unwrap();
    let assess = |sources: &[Source]| {
        assess_workspace_device(
            &fx.resolver,
            &Catalog::load(&fx.store).unwrap(),
            &fx.space,
            Some(&fx.project),
            &fx.card,
            sources,
            &finals.targets(),
        )
    };
    let excluded = assess(&[source.clone()]);
    assert!(excluded[0].1.status.wipe_eligible);
    assert_eq!(excluded[0].1.report.ignored, 1);
    assert_eq!(excluded[0].1.device_files[0].path, "DCIM/A.JPG");
    assert_eq!(excluded[0].1.device_files[0].state, SafeCopyState::Excluded);
    let parent = Source {
        id: "parent".into(),
        path_template: "".into(),
        safe_copy_rules: vec![],
        ..source.clone()
    };
    let overlap = assess(&[source, parent]);
    assert!(overlap
        .iter()
        .all(|(_, assessment)| !assessment.status.wipe_eligible));
    assert_eq!(overlap[0].1.device_files.len(), 1);
    assert_eq!(overlap[0].1.device_files[0].state, SafeCopyState::Unsafe);
    assert_eq!(overlap[0].1.report.ignored, 0);
}

#[test]
fn destination_exclusion_does_not_exempt_required_files_or_lower_threshold() {
    let fx = Fixture::new();
    fx.write_card_file("DCIM/A.JPG", b"a");
    fx.write_card_file("DCIM/B.JPG", b"b");
    copy(&fx, "a", "card", "DCIM/A.JPG");
    copy(&fx, "b", "card", "DCIM/B.JPG");
    copy(&fx, "a", "nas", "A.JPG");
    let mut destination = fx.destination.clone();
    destination.rules = vec![FileRule::path(
        RuleAction::Exclude,
        RuleSyntax::Glob,
        "B.JPG",
    )];
    fx.store.put(&destination).unwrap();
    let finals = FinalSet::load_for_space(&fx.store, "space").unwrap();
    let assessed = assess_workspace_device(
        &fx.resolver,
        &Catalog::load(&fx.store).unwrap(),
        &fx.space,
        Some(&fx.project),
        &fx.card,
        std::slice::from_ref(&fx.source),
        &finals.targets(),
    );
    let assessment = &assessed[0].1;
    assert_eq!(assessment.report.ignored, 0);
    assert_eq!(assessment.status.safe_copies, 0);
    assert!(!assessment.status.wipe_eligible);
    assert_eq!(assessment.device_files[1].state, SafeCopyState::Unsafe);
    assert!(assessment.device_files[1]
        .reasons
        .iter()
        .any(|reason| reason.contains("No eligible")));
    assert_eq!(assessment.device_files[0].state, SafeCopyState::Safe);
}

#[test]
fn acknowledgments_count_by_policy_but_are_never_reported_as_verified() {
    let fx = Fixture::new();
    fx.write_card_file("DCIM/A.JPG", b"a");
    copy(&fx, "a", "card", "DCIM/A.JPG");
    let file_id = FileRecord::id_for(HashAlgo::Xxh64, "a");
    fx.store
        .put(&crate::domain::SafeCopyOverride {
            id: "ack".into(),
            space_id: "space".into(),
            file_id,
            source_device_id: "card".into(),
            source_path: "DCIM/A.JPG".into(),
            destination_device_id: "nas".into(),
            destination_path: "A.JPG".into(),
            destination_hash: "different".into(),
        })
        .unwrap();
    let catalog = Catalog::load(&fx.store).unwrap();
    let finals = FinalSet::load_for_space(&fx.store, "space").unwrap();
    let assess = |space: &Space| {
        assess_workspace_device(
            &fx.resolver,
            &catalog,
            space,
            Some(&fx.project),
            &fx.card,
            std::slice::from_ref(&fx.source),
            &finals.targets(),
        )
    };
    let mut space = fx.space.clone();
    space.skip_counts_as_safe_copy = false;
    let disabled = assess(&space);
    assert!(!disabled[0].1.status.wipe_eligible);
    assert_eq!(disabled[0].1.device_files[0].safe_copies, 0);
    space.skip_counts_as_safe_copy = true;
    let enabled = assess(&space);
    let file = &enabled[0].1.device_files[0];
    assert!(enabled[0].1.status.wipe_eligible);
    assert_eq!(file.state, SafeCopyState::Safe);
    assert!(file.verified_destinations.is_empty());
    assert_eq!(file.acknowledged_destinations, vec!["Home NAS"]);
    assert!(file
        .reasons
        .iter()
        .any(|reason| reason.contains("not byte-verified")));
}

#[test]
fn missing_destination_in_this_space_and_invalid_rules_fail_conservatively() {
    let fx = Fixture::new();
    fx.write_card_file("DCIM/A.JPG", b"a");
    copy(&fx, "a", "card", "DCIM/A.JPG");
    copy(&fx, "a", "nas", "A.JPG");
    let mut destination = fx.destination.clone();
    destination.space_id = "elsewhere".into();
    fx.store.put(&destination).unwrap();
    let finals = FinalSet::load_for_space(&fx.store, "space").unwrap();
    let assessment = assess_workspace_source(
        &fx.resolver,
        &Catalog::load(&fx.store).unwrap(),
        &fx.space,
        Some(&fx.project),
        &fx.card,
        &fx.source,
        &finals.targets(),
    );
    assert_eq!(assessment.status.safe_copies, 0);
    assert!(!assessment.status.wipe_eligible);
    destination.space_id = "space".into();
    destination.rules = vec![FileRule::path(RuleAction::Exclude, RuleSyntax::Regex, "(")];
    fx.store.put(&destination).unwrap();
    assert!(FinalSet::load_for_space(&fx.store, "space").is_err());
}

#[test]
fn partial_destination_rule_views_do_not_manufacture_complete_copies() {
    let fx = Fixture::new();
    fx.write_card_file("DCIM/A.JPG", b"a");
    fx.write_card_file("DCIM/B.JPG", b"b");
    copy(&fx, "a", "card", "DCIM/A.JPG");
    copy(&fx, "b", "card", "DCIM/B.JPG");
    copy(&fx, "a", "nas", "A.JPG");
    let other = Device {
        id: "other".into(),
        name: "Other".into(),
        ..fx.nas.clone()
    };
    fx.store.put(&other).unwrap();
    copy(&fx, "b", "other", "B.JPG");
    let mut destination = fx.destination.clone();
    destination.rules = vec![FileRule::path(
        RuleAction::Include,
        RuleSyntax::Glob,
        "A.JPG",
    )];
    fx.store.put(&destination).unwrap();
    destination.id = "other-dest".into();
    destination.device_id = "other".into();
    destination.rules = vec![FileRule::path(
        RuleAction::Include,
        RuleSyntax::Glob,
        "B.JPG",
    )];
    fx.store.put(&destination).unwrap();
    let finals = FinalSet::load_for_space(&fx.store, "space").unwrap();
    let assessed = assess_workspace_device(
        &fx.resolver,
        &Catalog::load(&fx.store).unwrap(),
        &fx.space,
        Some(&fx.project),
        &fx.card,
        std::slice::from_ref(&fx.source),
        &finals.targets(),
    );
    assert_eq!(assessed[0].1.status.safe_copies, 0);
    assert!(!assessed[0].1.status.wipe_eligible);
    assert!(assessed[0]
        .1
        .device_files
        .iter()
        .all(|file| { file.safe_copies == 1 && file.state == SafeCopyState::Safe }));
}

#[test]
fn temporary_device_needs_a_real_final_destination_even_with_a_temporary_group() {
    let fx = Fixture::new();
    let mut space = fx.space.clone();
    space.temporary_copies_per_final = 2;
    let mut card = fx.card.clone();
    card.role = DeviceRole::Temporary;
    fx.write_card_file("DCIM/A.JPG", b"a");
    copy(&fx, "a", "card", "DCIM/A.JPG");
    let temporaries: Vec<_> = (0..2)
        .map(|i| Device {
            id: format!("temp{i}"),
            role: DeviceRole::Temporary,
            ..fx.nas.clone()
        })
        .collect();
    for device in &temporaries {
        copy(&fx, "a", &device.id, "A.JPG");
    }
    let targets: Vec<_> = temporaries
        .iter()
        .map(|device| FinalTarget {
            device,
            rules: vec![],
        })
        .collect();
    let assessed = assess_workspace_source(
        &fx.resolver,
        &Catalog::load(&fx.store).unwrap(),
        &space,
        Some(&fx.project),
        &card,
        &fx.source,
        &targets,
    );
    assert_eq!(assessed.status.safe_copies, 1);
    assert!(!assessed.status.wipe_eligible);
    assert_eq!(assessed.device_files[0].safe_copies, 1);
    assert_eq!(assessed.device_files[0].state, SafeCopyState::Unsafe);
    assert_eq!(
        assessed.status.blocking_reason.as_deref(),
        Some("Needs a final destination")
    );
}

#[test]
fn zero_threshold_still_requires_rule_coverage_and_source_hash_identity() {
    let fx = Fixture::new();
    let mut project = fx.project.clone();
    project.final_copies_required = 0;
    fx.write_card_file("DCIM/A.JPG", b"a");
    let catalog = Catalog::load(&fx.store).unwrap();
    let finals = FinalSet::load_for_space(&fx.store, "space").unwrap();
    let unknown = assess_workspace_source(
        &fx.resolver,
        &catalog,
        &fx.space,
        Some(&project),
        &fx.card,
        &fx.source,
        &finals.targets(),
    );
    assert!(!unknown.status.wipe_eligible);
    assert_eq!(unknown.device_files[0].state, SafeCopyState::Unsafe);
    let uncovered = assess_workspace_source(
        &fx.resolver,
        &catalog,
        &fx.space,
        Some(&project),
        &fx.card,
        &fx.source,
        &[],
    );
    assert!(!uncovered.status.wipe_eligible);
    assert!(uncovered
        .status
        .blocking_reason
        .unwrap()
        .contains("coverage"));
}

#[test]
fn sibling_sources_share_only_complete_device_coverage() {
    let fx = Fixture::new();
    sibling(&fx, "VIDEO");
    fx.write_card_file("DCIM/A.JPG", b"a");
    fx.write_card_file("VIDEO/B.MP4", b"b");
    copy(&fx, "a", "card", "DCIM/A.JPG");
    copy(&fx, "b", "card", "VIDEO/B.MP4");
    copy(&fx, "a", "nas", "A.JPG");
    let pending = status(&fx);
    assert!(pending
        .sources
        .iter()
        .all(|s| s.safe_copies == 0 && !s.wipe_eligible));
    assert!(pending.sources.iter().all(|s| s.file_count == 1));
    copy(&fx, "b", "nas", "B.MP4");
    assert!(status(&fx)
        .sources
        .iter()
        .all(|s| s.safe_copies == 1 && s.wipe_eligible));
    let mut duplicate = fx.destination.clone();
    duplicate.id = "second-folder-on-nas".into();
    fx.store.put(&duplicate).unwrap();
    assert!(status(&fx).sources.iter().all(|s| s.safe_copies == 1));
}

#[test]
fn disjoint_backup_devices_do_not_form_a_complete_copy() {
    let fx = Fixture::new();
    sibling(&fx, "VIDEO");
    let other = Device {
        id: "other".into(),
        role: DeviceRole::Final,
        ..fx.nas.clone()
    };
    fx.store.put(&other).unwrap();
    fx.write_card_file("DCIM/A.JPG", b"a");
    fx.write_card_file("VIDEO/B.MP4", b"b");
    copy(&fx, "a", "card", "DCIM/A.JPG");
    copy(&fx, "b", "card", "VIDEO/B.MP4");
    copy(&fx, "a", "nas", "A.JPG");
    copy(&fx, "b", "other", "B.MP4");
    assert!(status(&fx).sources.iter().all(|s| s.safe_copies == 0));
    copy(&fx, "b", "nas", "B.MP4");
    assert!(status(&fx).sources.iter().all(|s| s.safe_copies == 1));
}

#[test]
fn empty_sources_do_not_add_requirements_but_path_errors_block_the_device() {
    let fx = Fixture::new();
    let mut empty = sibling(&fx, "EMPTY");
    fx.write_card_file("DCIM/A.JPG", b"a");
    copy(&fx, "a", "card", "DCIM/A.JPG");
    copy(&fx, "a", "nas", "A.JPG");
    let complete = status(&fx);
    assert!(complete.sources[0].wipe_eligible);
    assert_eq!(complete.sources[1].safe_copies, 1);
    assert_eq!(
        complete.sources[1].blocking_reason.as_deref(),
        Some("No files")
    );
    empty.path_template = "{unknown}".into();
    fx.store.put(&empty).unwrap();
    let blocked = status(&fx);
    assert!(blocked.sources.iter().all(|s| !s.wipe_eligible));
    assert!(blocked.sources.iter().all(|s| s
        .blocking_reason
        .as_ref()
        .unwrap()
        .contains("sibling")));
}

#[test]
fn other_devices_and_spaces_do_not_join_the_group() {
    let fx = Fixture::new();
    let mut other_source = sibling(&fx, "VIDEO");
    other_source.device_id = fx.nas.id.clone();
    fx.store.put(&other_source).unwrap();
    let other_space = Space {
        id: "other-space".into(),
        ..fx.space.clone()
    };
    fx.store.put(&other_space).unwrap();
    let elsewhere = Source {
        id: "elsewhere".into(),
        space_id: other_space.id,
        path_template: "{unknown}".into(),
        ..fx.source.clone()
    };
    fx.store.put(&elsewhere).unwrap();
    fx.write_card_file("DCIM/A.JPG", b"a");
    copy(&fx, "a", "card", "DCIM/A.JPG");
    copy(&fx, "a", "nas", "A.JPG");
    let st = status(&fx);
    assert!(st.sources[0].wipe_eligible);
    assert_eq!(st.sources[0].safe_copies, 1);
    assert_eq!(st.sources[1].safe_copies, 0);
}

#[test]
fn overlapping_sources_keep_relative_rules_and_variables_and_deduplicate_paths() {
    let fx = Fixture::new();
    copy(&fx, "a", "card", "DCIM/A.JPG");
    copy(&fx, "a", "nas", "A.JPG");
    let catalog = Catalog::load(&fx.store).unwrap();
    let id = FileRecord::id_for(HashAlgo::Xxh64, "a");
    let parent_files = vec![("DCIM/A.JPG".into(), Some(id.clone()))];
    let child_files = vec![("A.JPG".into(), Some(id))];
    let parent_vars = TemplateVars::from([("source_name".into(), "Parent".into())]);
    let child_vars = TemplateVars::from([("source_name".into(), "Child".into())]);
    let rules = RuleSet::compile(&[
        FileRule::path(RuleAction::Include, RuleSyntax::Glob, "A.JPG"),
        FileRule::condition(RuleExpr::Eq {
            var: "source_name".into(),
            value: "Child".into(),
        }),
    ])
    .unwrap();
    let targets = [FinalTarget {
        device: &fx.nas,
        rules: vec![&rules],
    }];
    let sources = [
        SourceCopyFiles {
            folder: "",
            files: &parent_files,
            vars: &parent_vars,
            safe_copy_rules: None,
        },
        SourceCopyFiles {
            folder: "DCIM",
            files: &child_files,
            vars: &child_vars,
            safe_copy_rules: None,
        },
    ];
    let report = device_safe_copy_report(&sources, &targets, &catalog, 0, false, "space");
    assert_eq!(
        (report.files_total, report.ignored, report.safe_copies),
        (1, 0, 1)
    );
    assert_eq!((report.copies[0].verified, report.copies[0].total), (1, 1));
    let parent_only = device_safe_copy_report(&sources[..1], &targets, &catalog, 0, false, "space");
    assert_eq!((parent_only.ignored, parent_only.safe_copies), (0, 0));
}

#[test]
fn temporary_and_app_targets_require_group_coverage_and_never_count_self() {
    let fx = Fixture::new();
    sibling(&fx, "VIDEO");
    let mut space = fx.space.clone();
    space.temporary_copies_per_final = 2;
    fx.store.put(&space).unwrap();
    fx.write_card_file("DCIM/A.JPG", b"a");
    fx.write_card_file("VIDEO/B.MP4", b"b");
    copy(&fx, "a", "card", "DCIM/A.JPG");
    copy(&fx, "b", "card", "VIDEO/B.MP4");
    for i in 0..2 {
        let device = Device {
            id: format!("temp{i}"),
            role: DeviceRole::Temporary,
            ..fx.nas.clone()
        };
        let dest = Destination {
            id: format!("temp-dest{i}"),
            device_id: device.id.clone(),
            ..fx.destination.clone()
        };
        fx.store.put(&device).unwrap();
        fx.store.put(&dest).unwrap();
        copy(&fx, "a", &device.id, "A.JPG");
        if i == 0 {
            copy(&fx, "b", &device.id, "B.MP4");
        }
    }
    let app = Destination {
        id: "app".into(),
        kind: DestinationKind::App,
        device_id: String::new(),
        ..fx.destination.clone()
    };
    fx.store.put(&app).unwrap();
    copy(&fx, "a", "app", "A.JPG");
    assert!(status(&fx).sources.iter().all(|s| s.safe_copies == 0));
    copy(&fx, "b", "temp1", "B.MP4");
    assert!(status(&fx).sources.iter().all(|s| s.safe_copies == 1));
    copy(&fx, "b", "app", "B.MP4");
    assert!(status(&fx).sources.iter().all(|s| s.safe_copies == 2));
    let mut source_device = fx.card.clone();
    source_device.role = DeviceRole::Final;
    fx.store.put(&source_device).unwrap();
    assert!(status(&fx)
        .sources
        .iter()
        .all(|s| s.safe_copies == 2 && !s.wipe_eligible));
}
