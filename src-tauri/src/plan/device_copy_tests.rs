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
        },
        SourceCopyFiles {
            folder: "DCIM",
            files: &child_files,
            vars: &child_vars,
        },
    ];
    let report = device_safe_copy_report(&sources, &targets, &catalog, 0);
    assert_eq!(
        (report.files_total, report.ignored, report.safe_copies),
        (1, 0, 1)
    );
    assert_eq!((report.copies[0].verified, report.copies[0].total), (1, 1));
    let parent_only = device_safe_copy_report(&sources[..1], &targets, &catalog, 0);
    assert_eq!((parent_only.ignored, parent_only.safe_copies), (1, 0));
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
