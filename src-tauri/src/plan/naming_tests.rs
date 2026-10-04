use super::*;
use crate::domain::{Destination, DestinationKind, Device, Source, VariableDef};
use crate::testing::Fixture;

#[test]
fn legacy_names_default_empty_and_fall_back_to_physical_names() {
    let source: Source = serde_json::from_str(r#"{"id":"old-source","device_id":"card"}"#).unwrap();
    let mut destination: Destination =
        serde_json::from_str(r#"{"id":"old-destination","device_id":"nas"}"#).unwrap();
    let device = Device {
        name: "Physical device".into(),
        ..Default::default()
    };
    assert!(source.task_name.is_empty());
    assert!(source.backup_name.is_empty());
    assert!(destination.task_name.is_empty());
    assert_eq!(source.resolved_task_name(&device), device.name);
    assert_eq!(source.resolved_backup_name(&device), device.name);
    assert_eq!(destination.resolved_task_name(&device), device.name);
    destination.kind = DestinationKind::App;
    destination.app_name = Some("  Lightroom  ".into());
    assert_eq!(destination.resolved_task_name(&device), "Lightroom");
    destination.app_name = Some(" \n ".into());
    assert_eq!(destination.resolved_task_name(&device), "Application");
}

#[test]
fn whitespace_overrides_fall_back_and_nonblank_overrides_are_trimmed() {
    let device = Device {
        name: "Physical".into(),
        ..Default::default()
    };
    let mut source = Source {
        task_name: " \n ".into(),
        backup_name: " \t ".into(),
        ..Default::default()
    };
    let mut destination = Destination {
        task_name: " \n ".into(),
        ..Default::default()
    };
    assert_eq!(source.resolved_task_name(&device), "Physical");
    assert_eq!(source.resolved_backup_name(&device), "Physical");
    assert_eq!(destination.resolved_task_name(&device), "Physical");
    source.task_name = "  Import  ".into();
    source.backup_name = "  Camera A  ".into();
    destination.task_name = "  Archive  ".into();
    assert_eq!(source.resolved_task_name(&device), "Import");
    assert_eq!(source.resolved_backup_name(&device), "Camera A");
    assert_eq!(destination.resolved_task_name(&device), "Archive");
}

#[test]
fn independent_sources_sharing_device_keep_separate_names() {
    let fx = Fixture::new();
    let mut first = fx.source.clone();
    first.task_name = "  Photos import  ".into();
    first.backup_name = "Photos".into();
    let mut second = first.clone();
    second.id = "second-source".into();
    second.task_name = "Movies import".into();
    second.backup_name = "Movies".into();
    fx.store.put_all(&[first, second.clone()]).unwrap();
    let mut second_flow = fx.flow.clone();
    second_flow.id = "second-flow".into();
    second_flow.source_id = second.id.clone();
    fx.store.put(&second_flow).unwrap();
    let first_ctx = resolve_flow(&fx.store, &fx.resolver, "project", "flow").unwrap();
    let second_ctx = resolve_flow(&fx.store, &fx.resolver, "project", &second_flow.id).unwrap();
    assert_eq!(first_ctx.label(), "Photos import → Home NAS");
    assert_eq!(second_ctx.label(), "Movies import → Home NAS");
    assert_eq!(first_ctx.vars["source_name"], "Photos");
    assert_eq!(second_ctx.vars["source_name"], "Movies");
    assert_eq!(first_ctx.target_rel("A.JPG"), "photo/Trip/Photos/A.JPG");
    assert_eq!(second_ctx.target_rel("A.JPG"), "photo/Trip/Movies/A.JPG");
    assert_eq!(
        fx.store.get::<Device>(&fx.card.id).unwrap().unwrap(),
        fx.card
    );
    assert_eq!(first_ctx.source_device.id, second_ctx.source_device.id);
}

#[test]
fn task_renames_change_only_labels_not_paths_or_identity() {
    let fx = Fixture::new();
    let before = resolve_flow(&fx.store, &fx.resolver, "project", "flow").unwrap();
    let mut source = fx.source.clone();
    source.task_name = "  Ingest / display only  ".into();
    let mut destination = fx.destination.clone();
    destination.task_name = "  Final / display only  ".into();
    fx.store.put(&source).unwrap();
    fx.store.put(&destination).unwrap();
    let after = resolve_flow(&fx.store, &fx.resolver, "project", "flow").unwrap();
    assert_eq!(
        after.label(),
        "Ingest / display only → Final / display only"
    );
    assert_eq!(before.source_folder(), after.source_folder());
    assert_eq!(before.target_abs("A.JPG"), after.target_abs("A.JPG"));
    assert_eq!(
        before
            .target_for_project("A.JPG", Some(&fx.project))
            .unwrap(),
        after
            .target_for_project("A.JPG", Some(&fx.project))
            .unwrap()
    );
    assert_eq!(before.vars, after.vars);
    assert_eq!(before.flow, after.flow);
    assert_eq!(before.source.device_id, after.source.device_id);
    assert_eq!(before.destination.device_id, after.destination.device_id);
    assert_eq!(
        fx.store.get::<Device>(&fx.card.id).unwrap().unwrap(),
        fx.card
    );
    assert_eq!(fx.store.get::<Device>(&fx.nas.id).unwrap().unwrap(), fx.nas);
}

#[test]
fn backup_override_is_sanitized_in_paths_assessment_rules_and_markers() {
    let fx = Fixture::new();
    let mut source = fx.source.clone();
    source.backup_name = "  ..Camera/A\\B:*?\"<>|\n..  ".into();
    source.task_name = "Different label".into();
    source.path_template = "{source_name}/DCIM".into();
    let expected = "Camera_A_B________";
    let mut destination = fx.destination.clone();
    destination.path_template = "{source_name}/{backup_folder}".into();
    destination.use_backup_marker = true;
    destination.rules = vec![crate::domain::FileRule::condition(
        crate::domain::RuleExpr::Eq {
            var: "source_name".into(),
            value: expected.into(),
        },
    )];
    let mut space = fx.space.clone();
    space.backup_marker_template = "{source_name}-backup".into();
    fx.store.put(&source).unwrap();
    fx.store.put(&destination).unwrap();
    fx.store.put(&space).unwrap();
    fx.write_card_file(&format!("{expected}/DCIM/A.JPG"), b"photo");
    let ctx = resolve_flow(&fx.store, &fx.resolver, "project", "flow").unwrap();
    assert!(ctx.config_error.is_none(), "{:?}", ctx.config_error);
    assert_eq!(ctx.vars["source_name"], expected);
    assert_eq!(ctx.vars["backup_folder"], format!("{expected}-backup"));
    assert_eq!(
        ctx.rule_vars_for_project(Some(&fx.project))["source_name"],
        expected
    );
    assert!(ctx.rule_vars_for_project(None).is_empty());
    assert!(ctx
        .rules
        .allows_with_vars("A.JPG", &ctx.rule_vars_for_project(Some(&fx.project))));
    let target = format!("{expected}/{expected}-backup/{expected}/A.JPG");
    assert_eq!(ctx.target_rel("A.JPG"), target);
    assert_eq!(
        ctx.target_for_project("A.JPG", Some(&fx.project)).unwrap(),
        Some(target)
    );
    let assessment = assess_source(
        &fx.resolver,
        &Catalog::load(&fx.store).unwrap(),
        &space,
        &fx.project,
        &fx.card,
        &source,
        &[],
    );
    assert_eq!(assessment.folder, format!("{expected}/DCIM"));
    assert_eq!(assessment.status.file_count, 1);
    destination.subfolder_per_source = false;
    fx.store.put(&destination).unwrap();
    let ctx = resolve_flow(&fx.store, &fx.resolver, "project", "flow").unwrap();
    assert_eq!(
        ctx.target_for_project("A.JPG", None).unwrap(),
        Some(format!("{expected}/{expected}-backup/A.JPG"))
    );
}

#[test]
fn source_name_retains_space_then_project_override_precedence() {
    let fx = Fixture::new();
    let mut source = fx.source.clone();
    source.backup_name = "Backup name".into();
    let mut space = fx.space.clone();
    space.variables.push(VariableDef {
        name: "source_name".into(),
        default_value: "Space value".into(),
        ..Default::default()
    });
    let mut project = fx.project.clone();
    assert_eq!(
        source_template_vars(&space, None, &fx.card, &source)["source_name"],
        "Backup name"
    );
    assert_eq!(
        source_template_vars(&space, Some(&project), &fx.card, &source)["source_name"],
        "Space value"
    );
    project
        .values
        .insert("source_name".into(), "Project value".into());
    assert_eq!(
        source_template_vars(&space, Some(&project), &fx.card, &source)["source_name"],
        "Project value"
    );
    fx.store.put(&source).unwrap();
    fx.store.put(&space).unwrap();
    fx.store.put(&project).unwrap();
    let mut destination = fx.destination.clone();
    destination.path_template = "{source_name}".into();
    fx.store.put(&destination).unwrap();
    let ctx = resolve_flow(&fx.store, &fx.resolver, "project", "flow").unwrap();
    assert_eq!(
        ctx.target_for_project("A.JPG", Some(&project)).unwrap(),
        Some("Project value/Backup name/A.JPG".into())
    );
    assert_eq!(
        ctx.rule_vars_for_project(Some(&project))["source_name"],
        "Project value"
    );
}

#[test]
fn existing_marker_remains_authoritative_after_backup_rename() {
    let fx = Fixture::new();
    crate::paths::ensure_backup_folder(fx.card_dir.path(), "Original backup").unwrap();
    fx.write_card_file("Original backup/A.JPG", b"existing backup");
    let mut source = fx.source.clone();
    source.backup_name = "New name".into();
    let mut destination = fx.destination.clone();
    destination.path_template = "{backup_folder}".into();
    destination.use_backup_marker = true;
    destination.subfolder_per_source = false;
    let mut space = fx.space.clone();
    space.backup_marker_template = "{source_name}".into();
    fx.store.put(&source).unwrap();
    fx.store.put(&destination).unwrap();
    fx.store.put(&space).unwrap();
    let ctx = resolve_flow(&fx.store, &fx.resolver, "project", "flow").unwrap();
    assert_eq!(ctx.vars["backup_folder"], "Original backup");
    assert_eq!(
        ctx.target_for_project("A.JPG", Some(&fx.project)).unwrap(),
        Some("Original backup/A.JPG".into())
    );
    assert_eq!(
        crate::paths::read_backup_folder(fx.card_dir.path()).as_deref(),
        Some("Original backup")
    );
    assert!(fx.card_dir.path().join("Original backup/A.JPG").exists());
    assert!(!fx.card_dir.path().join("New name").exists());
}

#[test]
fn app_destination_labels_use_task_override_without_changing_targets() {
    let fx = Fixture::new();
    let mut destination = fx.destination.clone();
    destination.kind = DestinationKind::App;
    destination.device_id.clear();
    destination.app_name = Some("  Lightroom  ".into());
    fx.store.put(&destination).unwrap();
    let before = resolve_flow(&fx.store, &fx.resolver, "project", "flow").unwrap();
    assert_eq!(before.label(), "Camera A Card 1 → Lightroom");
    destination.task_name = "  Edit photos  ".into();
    fx.store.put(&destination).unwrap();
    let after = resolve_flow(&fx.store, &fx.resolver, "project", "flow").unwrap();
    assert_eq!(after.label(), "Camera A Card 1 → Edit photos");
    assert_eq!(after.dest_device.name, "Lightroom");
    assert_eq!(before.dest_device.id, after.dest_device.id);
    assert_eq!(
        before
            .target_for_project("A.JPG", Some(&fx.project))
            .unwrap(),
        after
            .target_for_project("A.JPG", Some(&fx.project))
            .unwrap()
    );
}

#[test]
fn transfer_creates_backup_marker_and_task_rename_keeps_existing_catalog_and_files() {
    use crate::domain::{FileCopy, FileRecord};
    use crate::transfer::{run_transfer, JobHandle, JobState, TransferJob};
    use parking_lot::Mutex;
    use std::sync::Arc;

    let fx = Fixture::new();
    fx.write_card_file("DCIM/A.JPG", b"photo");
    let mut source = fx.source.clone();
    source.task_name = "Import".into();
    source.backup_name = " Camera/A ".into();
    let mut destination = fx.destination.clone();
    destination.task_name = "Archive".into();
    destination.path_template = "{backup_folder}".into();
    destination.use_backup_marker = true;
    let mut space = fx.space.clone();
    space.backup_marker_template = "{source_name}-backup".into();
    fx.store.put(&source).unwrap();
    fx.store.put(&destination).unwrap();
    fx.store.put(&space).unwrap();
    let run = || {
        let handle = JobHandle::new(
            TransferJob {
                id: "job".into(),
                flow_id: fx.flow.id.clone(),
                label: String::new(),
                state: JobState::Queued,
                files_done: 0,
                files_total: 0,
                bytes_done: 0,
                bytes_total: 0,
                current_file: None,
                speed_bps: 0,
                bytes_per_sec: None,
                eta_secs: None,
                errors: vec![],
            },
            Arc::new(|| {}),
        );
        run_transfer(
            &fx.store,
            &fx.resolver,
            &fx.project.id,
            &fx.flow.id,
            &handle,
            &Mutex::new(FailureMap::new()),
        )
        .unwrap();
        handle.snapshot()
    };
    let first = run();
    assert_eq!(first.label, "Import → Archive");
    assert_eq!(first.files_total, 1);
    assert_eq!(
        crate::paths::read_backup_folder(fx.card_dir.path()).as_deref(),
        Some("Camera_A-backup")
    );
    let target = fx.nas_dir.path().join("Camera_A-backup/Camera_A/A.JPG");
    assert_eq!(std::fs::read(&target).unwrap(), b"photo");
    let copies = fx.store.list::<FileCopy>().unwrap();
    let records = fx.store.list::<FileRecord>().unwrap();
    assert!(copies
        .iter()
        .any(|copy| copy.device_id == fx.nas.id && copy.path == "Camera_A-backup/Camera_A/A.JPG"));
    source.task_name = "Renamed import".into();
    destination.task_name = "Renamed archive".into();
    fx.store.put(&source).unwrap();
    fx.store.put(&destination).unwrap();
    let second = run();
    assert_eq!(second.label, "Renamed import → Renamed archive");
    assert_eq!(second.files_total, 0);
    assert_eq!(fx.store.list::<FileCopy>().unwrap(), copies);
    assert_eq!(fx.store.list::<FileRecord>().unwrap(), records);
    assert_eq!(std::fs::read(&target).unwrap(), b"photo");
    assert_eq!(
        crate::paths::read_backup_folder(fx.card_dir.path()).as_deref(),
        Some("Camera_A-backup")
    );
}
