use super::*;
use crate::domain::{Destination, DestinationKind, Flow, Source};
use crate::plan::Category;
use crate::testing::{Fixture, MapResolver};
use serde_json::json;
use std::fs;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn core(fx: &Fixture) -> (AppCore, tempfile::TempDir) {
    let roots = fx.resolver.0.lock().clone();
    let thumbs = tempfile::tempdir().unwrap();
    let core = AppCore::new(
        fx.store.clone(),
        Arc::new(MapResolver(parking_lot::Mutex::new(roots))),
        thumbs.path().into(),
        |_| {},
    );
    (core, thumbs)
}

fn wait_idle(core: &AppCore) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while core.transfers.is_busy() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn snapshot_and_settings_round_trip() {
    let fx = Fixture::new();
    let (core, _t) = core(&fx);
    core.register_computer().unwrap();
    let snap = core.snapshot().unwrap();
    assert_eq!(snap.computer.id, fx.store.computer_id());
    assert_eq!(snap.spaces.len(), 1);
    let mut settings = core.settings();
    settings.theme = "dark".into();
    core.save_settings(&settings).unwrap();
    assert_eq!(core.settings().theme, "dark");
}

#[test]
fn delete_cascades_and_guards_devices() {
    let fx = Fixture::new();
    let (core, _t) = core(&fx);
    assert!(core.delete_entity("device", "card").is_err());
    core.delete_entity("source", "src").unwrap();
    assert!(fx.store.list::<Flow>().unwrap().is_empty());
    core.delete_entity("space", "space").unwrap();
    assert!(core.snapshot().unwrap().destinations.is_empty());
    core.delete_entity("device", "card").unwrap();
}

#[test]
fn save_entity_validates() {
    let fx = Fixture::new();
    let (core, _t) = core(&fx);
    core.save_entity(
        "source",
        json!({"id": "s2", "space_id": "space", "device_id": "card"}),
    )
    .unwrap();
    assert!(fx.store.get::<Source>("s2").unwrap().is_some());
    assert!(core.save_entity("file_copy", json!({"id": "x"})).is_err());
    assert!(core
        .save_entity("source", json!({"space_id": "space"}))
        .is_err());
}

#[test]
fn run_all_then_list_files() {
    let fx = Fixture::new();
    fx.write_card_file("DCIM/A.JPG", b"a");
    fx.write_card_file("DCIM/B.MOV", b"bb");
    fx.write_card_file("DCIM/C.JPG", b"c");
    let (core, _t) = core(&fx);
    let req = |category, offset, limit| ListFilesRequest {
        project_id: "project".into(),
        flow_id: "flow".into(),
        category,
        offset,
        limit,
        filter: None,
    };
    let page = core.list_files(&req(Category::ToTransfer, 1, 1)).unwrap();
    assert_eq!((page.total, page.total_bytes, page.items.len()), (3, 4, 1));
    assert_eq!(
        page.items[0].target_path.as_deref(),
        Some("photo/Trip/Camera A Card 1/B.MOV")
    );
    assert_eq!(core.run_all("project").unwrap().len(), 1);
    wait_idle(&core);
    assert_eq!(
        core.list_files(&req(Category::Transferred, 0, 50))
            .unwrap()
            .total,
        3
    );
    assert!(core.run_all("project").unwrap().is_empty());
    let status = core.project_status("project").unwrap();
    assert!(status.sources[0].wipe_eligible);
    core.start_wipe("project", "src", crate::wipe::WipeMethod::DeleteFiles)
        .unwrap();
    wait_idle(&core);
    assert!(!fx.card_dir.path().join("DCIM/A.JPG").exists());
}

#[test]
fn project_free_transfers_preview_verify_and_record_copies_without_enabling_wipe() {
    let fx = Fixture::new();
    fx.store
        .delete(crate::domain::EntityKind::Project, "project")
        .unwrap();
    let source = fx.write_card_file("DCIM/A.JPG", b"project-free photo");
    let (core, _t) = core(&fx);
    let context = crate::plan::WorkspaceContext {
        space_id: fx.space.id.clone(),
        project_id: None,
    };
    let req = |category| ListWorkspaceFilesRequest {
        context: context.clone(),
        flow_id: fx.flow.id.clone(),
        category,
        offset: 0,
        limit: 50,
        filter: None,
    };
    let pending = core
        .list_workspace_files(&req(Category::ToTransfer))
        .unwrap();
    assert_eq!(pending.total, 1);
    assert_eq!(pending.items[0].project_id, None);
    assert_eq!(
        core.authorize_media_open(&source).unwrap(),
        source.canonicalize().unwrap()
    );
    assert_eq!(core.run_workspace_all(&context).unwrap().len(), 1);
    wait_idle(&core);
    assert!(core
        .transfers
        .jobs()
        .iter()
        .all(|j| j.state == crate::transfer::JobState::Done));
    let target = fx.nas_dir.path().join("photo/Trip/Camera A Card 1/A.JPG");
    assert_eq!(fs::read(&target).unwrap(), b"project-free photo");
    assert_eq!(
        crate::hashing::hash_file(&source, fx.space.hash_algo, |_| true).unwrap(),
        crate::hashing::hash_file(&target, fx.space.hash_algo, |_| true).unwrap(),
    );
    let records = fx.store.list::<crate::domain::FileRecord>().unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(fx.store.list::<crate::domain::FileCopy>().unwrap().len(), 2);
    assert_eq!(
        core.list_workspace_files(&req(Category::Transferred))
            .unwrap()
            .total,
        1
    );
    let status = core.workspace_status(&context).unwrap();
    assert_eq!(status.sources[0].safe_copies, 1);
    assert_eq!(status.sources[0].required_copies, None);
    assert!(!status.sources[0].wipe_eligible);
    assert!(core
        .start_wipe("", "src", crate::wipe::WipeMethod::DeleteFiles)
        .is_err());
    assert!(core
        .start_wipe("project", "src", crate::wipe::WipeMethod::DeleteFiles)
        .is_err());
    assert!(core.run_workspace_all(&context).unwrap().is_empty());
    assert!(fx
        .store
        .list::<crate::domain::Project>()
        .unwrap()
        .is_empty());
}

#[test]
fn project_free_blocked_paths_never_copy_or_create_markers() {
    let fx = Fixture::new();
    fx.store
        .delete(crate::domain::EntityKind::Project, "project")
        .unwrap();
    fx.write_card_file("DCIM/A.JPG", b"photo");
    let mut destination = fx.destination.clone();
    destination.path_template = "{backup_folder}".into();
    destination.use_backup_marker = true;
    fx.store.put(&destination).unwrap();
    let (core, _t) = core(&fx);
    let context = crate::plan::WorkspaceContext {
        space_id: "space".into(),
        project_id: None,
    };
    assert!(core
        .run_workspace_flow(&context, "flow")
        .unwrap_err()
        .contains("backup folder"));
    assert!(core.run_workspace_all(&context).unwrap().is_empty());
    assert!(!fx
        .card_dir
        .path()
        .join(crate::paths::BACKUP_MARKER_FILE)
        .exists());
    assert!(fs::read_dir(fx.nas_dir.path()).unwrap().next().is_none());
    crate::paths::ensure_backup_folder(fx.card_dir.path(), "Existing").unwrap();
    core.run_workspace_flow(&context, "flow").unwrap();
    wait_idle(&core);
    assert_eq!(
        fs::read(fx.nas_dir.path().join("Existing/Camera A Card 1/A.JPG")).unwrap(),
        b"photo"
    );
}

#[test]
fn project_free_app_imports_are_manual_and_confirmations_are_context_bound() {
    let fx = Fixture::new();
    fx.store
        .delete(crate::domain::EntityKind::Project, "project")
        .unwrap();
    fx.write_card_file("DCIM/A.JPG", b"photo");
    let mut destination = fx.destination.clone();
    destination.kind = DestinationKind::App;
    destination.device_id.clear();
    destination.path_template.clear();
    destination.app_name = Some("Photo app".into());
    destination.counts_as_safe_copy = false;
    fx.store.put(&destination).unwrap();
    let (core, _t) = core(&fx);
    let app_path = fake_app(fx.card_dir.path());
    let mut settings = core.settings();
    settings.app_destinations.insert(
        destination.id.clone(),
        app_path.to_string_lossy().into_owned(),
    );
    core.save_settings(&settings).unwrap();
    let context = crate::plan::WorkspaceContext {
        space_id: "space".into(),
        project_id: None,
    };
    assert!(core.run_workspace_all(&context).unwrap().is_empty());
    assert!(core
        .run_workspace_flow(&context, "flow")
        .unwrap_err()
        .contains("manually"));
    let prepared = core.prepare_workspace_app_import(&context, "flow").unwrap();
    assert_eq!(prepared.files[0].project_id, None);
    assert_eq!(
        core.confirm_workspace_app_import(&context, "flow", &prepared.token)
            .unwrap(),
        1
    );
    assert_eq!(
        core.workspace_status(&context).unwrap().destinations[0].transferred,
        1
    );
    assert_eq!(
        core.workspace_status(&context).unwrap().sources[0].safe_copies,
        0
    );
    assert!(core
        .confirm_workspace_app_import(&context, "flow", &prepared.token)
        .is_err());
    fx.write_card_file("DCIM/B.JPG", b"new photo");
    let prepared = core.prepare_workspace_app_import(&context, "flow").unwrap();
    let other = crate::plan::WorkspaceContext {
        space_id: "other".into(),
        project_id: None,
    };
    assert!(core
        .confirm_workspace_app_import(&other, "flow", &prepared.token)
        .unwrap_err()
        .contains("does not match"));
}

#[test]
fn project_free_transfer_can_generate_a_project_independent_marker() {
    let fx = Fixture::new();
    fx.store
        .delete(crate::domain::EntityKind::Project, "project")
        .unwrap();
    fx.write_card_file("DCIM/A.JPG", b"photo");
    let mut space = fx.space.clone();
    space.backup_marker_template = "{date}".into();
    fx.store.put(&space).unwrap();
    let mut destination = fx.destination.clone();
    destination.path_template = "{backup_folder}".into();
    destination.use_backup_marker = true;
    destination.subfolder_per_source = false;
    fx.store.put(&destination).unwrap();
    let (core, _t) = core(&fx);
    let context = crate::plan::WorkspaceContext {
        space_id: "space".into(),
        project_id: None,
    };
    assert!(core.workspace_status(&context).unwrap().flows[0].runnable);
    core.run_workspace_flow(&context, "flow").unwrap();
    wait_idle(&core);
    let folder = crate::paths::read_backup_folder(fx.card_dir.path()).unwrap();
    assert_eq!(
        fs::read(fx.nas_dir.path().join(folder).join("A.JPG")).unwrap(),
        b"photo"
    );
    assert_eq!(
        core.workspace_status(&context).unwrap().destinations[0].transferred,
        1
    );
}

#[test]
fn destination_kind_defaults_to_folder_for_old_snapshots() {
    let destination: Destination = serde_json::from_value(json!({
        "id": "dst",
        "space_id": "space",
        "device_id": "nas",
        "path_template": "photo",
        "app_path": "/old/local/path"
    }))
    .unwrap();
    assert_eq!(destination.kind, DestinationKind::Folder);
}

#[test]
fn app_destinations_are_excluded_from_run_all() {
    let fx = Fixture::new();
    fx.write_card_file("DCIM/A.JPG", b"a");
    let mut destination = fx.destination.clone();
    destination.kind = DestinationKind::App;
    destination.device_id.clear();
    destination.path_template.clear();
    destination.app_name = Some("Lightroom".into());
    destination.counts_as_safe_copy = false;
    fx.store.put(&destination).unwrap();
    let (core, _t) = core(&fx);
    assert!(core
        .run_flow("project", "flow")
        .unwrap_err()
        .contains("manually"));
    assert!(core.run_all("project").unwrap().is_empty());
}

#[test]
fn confirming_app_import_only_marks_current_to_transfer_files() {
    let fx = Fixture::new();
    fx.write_card_file("DCIM/A.JPG", b"a");
    fx.write_card_file("DCIM/B.MOV", b"bb");
    let mut destination = fx.destination.clone();
    destination.kind = DestinationKind::App;
    destination.device_id.clear();
    destination.path_template.clear();
    destination.app_name = Some("Lightroom".into());
    destination.counts_as_safe_copy = false;
    destination.rules = vec![crate::domain::FileRule::path(
        crate::domain::RuleAction::Include,
        crate::domain::RuleSyntax::Glob,
        "*.JPG",
    )];
    fx.store.put(&destination).unwrap();
    let (core, _t) = core(&fx);
    let err = core.prepare_app_import("project", "flow").unwrap_err();
    assert_eq!(err, "Choose the application for Lightroom on this computer");
    let app_path = fake_app(fx.card_dir.path());
    let mut settings = core.settings();
    settings.app_destinations.insert(
        destination.id.clone(),
        app_path.to_string_lossy().into_owned(),
    );
    core.save_settings(&settings).unwrap();
    let prepared = core.prepare_app_import("project", "flow").unwrap();
    assert_eq!(prepared.app_name, "Lightroom");
    assert_eq!(
        prepared.app_path,
        app_path
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .into_owned()
    );
    assert_eq!(prepared.paths.len(), 1);
    assert_eq!(prepared.files[0].rel_path, "A.JPG");

    destination.rules.clear();
    destination.rules.push(crate::domain::FileRule::path(
        crate::domain::RuleAction::Exclude,
        crate::domain::RuleSyntax::Glob,
        "*.JPG",
    ));
    fx.store.put(&destination).unwrap();
    assert_eq!(
        core.confirm_app_import("project", "flow", &prepared.token)
            .unwrap(),
        0
    );
    assert_eq!(
        core.list_files(&ListFilesRequest {
            project_id: "project".into(),
            flow_id: "flow".into(),
            category: Category::Transferred,
            offset: 0,
            limit: 50,
            filter: None,
        })
        .unwrap()
        .total,
        0
    );
}

#[test]
fn save_project_policy_rejects_overlap_and_none_scope_without_mutation() {
    let fx = Fixture::new();
    let (core, _t) = core(&fx);
    let mut project = fx.project.clone();
    project.start_time = Some(0);
    project.end_time = Some(60_000);
    core.save_entity("project", serde_json::to_value(&project).unwrap())
        .unwrap();
    project.id = "second".into();
    core.save_entity("project", serde_json::to_value(&project).unwrap())
        .unwrap();
    let mut space = fx.space.clone();
    space.allow_project_overlap = false;
    assert!(core
        .save_entity("space", serde_json::to_value(&space).unwrap())
        .unwrap_err()
        .contains("overlap"));
    assert!(
        fx.store
            .get::<crate::domain::Space>("space")
            .unwrap()
            .unwrap()
            .allow_project_overlap
    );

    let mut destination = fx.destination.clone();
    destination.path_template = "{project_name}".into();
    core.save_entity("destination", serde_json::to_value(&destination).unwrap())
        .unwrap();
    let mut source = fx.source.clone();
    source.project_scope = crate::domain::ProjectScope::None;
    assert!(core
        .save_entity("source", serde_json::to_value(&source).unwrap())
        .unwrap_err()
        .contains("cannot use project variables"));
    assert_eq!(
        fx.store
            .get::<Source>("src")
            .unwrap()
            .unwrap()
            .project_scope,
        crate::domain::ProjectScope::All
    );
}

fn fake_app(dir: &std::path::Path) -> std::path::PathBuf {
    #[cfg(target_os = "macos")]
    {
        let path = dir.join("Lightroom.app");
        fs::create_dir_all(&path).unwrap();
        path
    }
    #[cfg(target_os = "windows")]
    {
        let path = dir.join("Lightroom.exe");
        fs::write(&path, b"").unwrap();
        path
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let path = dir.join("lightroom.desktop");
        fs::write(&path, b"").unwrap();
        path
    }
}
