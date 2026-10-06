//! End-to-end tests through the real Tauri IPC layer, using the same JSON payloads as `src/api/tauri-backend.ts`
//! and `src/state/factories.ts`, against real folders standing in for a card and a NAS.
use serde_json::{json, Value};
use std::time::{Duration, Instant};
use tauri::ipc::{CallbackFn, InvokeBody, InvokeResponseBody};
use tauri::test::{get_ipc_response, mock_builder, INVOKE_KEY};
use tauri::webview::InvokeRequest;
use tauri::{WebviewWindow, WebviewWindowBuilder};

struct Ui {
    webview: WebviewWindow<tauri::test::MockRuntime>,
    _app: tauri::App<tauri::test::MockRuntime>,
    _data: tempfile::TempDir,
}

impl Ui {
    fn start() -> Self {
        let data = tempfile::tempdir().unwrap();
        let app = mock_builder()
            .invoke_handler(crate::omb_handlers!())
            .build(crate::context())
            .unwrap();
        super::init(app.handle(), data.path()).unwrap();
        let webview = WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        Self {
            webview,
            _app: app,
            _data: data,
        }
    }

    fn call(&self, cmd: &str, args: Value) -> Result<Value, Value> {
        get_ipc_response(&self.webview, self.request(cmd, args))
            .map(|b| b.deserialize::<Value>().unwrap())
    }

    fn request(&self, cmd: &str, args: Value) -> InvokeRequest {
        InvokeRequest {
            cmd: cmd.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: "http://localhost:1420".parse().unwrap(),
            body: InvokeBody::Json(args),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        }
    }

    fn ok(&self, cmd: &str, args: Value) -> Value {
        self.call(cmd, args)
            .unwrap_or_else(|e| panic!("{cmd} failed: {e}"))
    }

    fn save(&self, kind: &str, entity: Value) {
        self.ok("save_entity", json!({ "kind": kind, "entity": entity }));
    }

    fn wait_idle(&self) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let jobs = self.ok("list_transfers", json!({}));
            let busy = jobs.as_array().unwrap().iter().any(|j| {
                matches!(
                    j["state"].as_str(),
                    Some("queued" | "running" | "verifying")
                )
            });
            if !busy || Instant::now() > deadline {
                return;
            }
            std::thread::sleep(Duration::from_millis(30));
        }
    }

    fn flow_status(&self, project: &str) -> Value {
        self.ok("get_project_status", json!({ "projectId": project }))["flows"][0].clone()
    }
}

fn write(root: &std::path::Path, rel: &str, bytes: &[u8]) {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

#[test]
fn manual_wipe_ipc_accepts_only_a_stored_source_id_and_persists_removal() {
    use crate::domain::{Device, FileCopy, Source};
    use tauri::Manager;
    let ui = Ui::start();
    let state = ui.webview.state::<super::Shared>();
    state
        .core
        .store
        .put(&Device {
            id: "card".into(),
            ..Default::default()
        })
        .unwrap();
    state
        .core
        .store
        .put(&Source {
            id: "src".into(),
            device_id: "card".into(),
            ..Default::default()
        })
        .unwrap();
    state
        .core
        .store
        .put(&FileCopy {
            id: FileCopy::id_for("hash", "card", "OLD.JPG"),
            file_id: "hash".into(),
            device_id: "card".into(),
            path: "OLD.JPG".into(),
            ..Default::default()
        })
        .unwrap();
    assert!(ui
        .call(
            "mark_source_manually_wiped",
            json!({"sourceId": "/arbitrary/path"})
        )
        .is_err());
    ui.ok("mark_source_manually_wiped", json!({"sourceId": "src"}));
    assert!(state.core.store.list::<FileCopy>().unwrap()[0].removed);
}

#[test]
fn speed_analysis_ipc_exact_snake_case_contract_and_validation() {
    use crate::transfer::metrics::{AnalysisKind, AnalysisState};
    use crate::transfer::{AnalysisJob, AnalysisMetrics, AnalysisPhase};
    use tauri::Manager;

    let ui = Ui::start();
    let filter = json!({"space_id": null, "since": null, "pair_id": null});
    let empty = ui.ok("get_speed_analysis", json!({"req": filter}));
    assert_eq!(empty["pairs"], json!([]));
    assert!(empty["totals"]["avg_copy_bps"].is_null());
    assert!(empty["totals"]["effective_bps"].is_null());
    let context = crate::transfer::metrics::tests::context();
    let job = AnalysisJob {
        id: "analysis-ipc".into(),
        context: context.clone(),
        kind: AnalysisKind::Transfer,
        state: AnalysisState::Done,
        phase: AnalysisPhase::Finished,
        created_at: 1000,
        updated_at: 5000,
        finished_at: Some(5000),
        metrics: AnalysisMetrics {
            copy_secs: 2.0,
            other_secs: 1.0,
            destination_check_secs: 1.0,
            copy_bytes: 10,
            committed_bytes: 8,
            transferred_files: 1,
            ..Default::default()
        },
        error_count: 0,
    };
    ui.webview
        .state::<super::Shared>()
        .core
        .store
        .save_analysis_job(&job)
        .unwrap();
    let summary = ui.ok("get_speed_analysis", json!({"req": filter}));
    assert_eq!(summary["totals"]["completed_transfer_jobs"], 1);
    assert_eq!(summary["totals"]["avg_copy_bps"], 5.0);
    assert_eq!(summary["totals"]["effective_bps"], 2.0);
    assert_eq!(
        summary["pairs"][0]["context"],
        serde_json::to_value(context).unwrap()
    );
    let page = ui.ok("list_speed_analysis_jobs", json!({
        "req": {"space_id": "s", "since": 1000, "pair_id": job.context.pair_id, "offset": 0, "limit": 100}
    }));
    assert_eq!(page["total"], 1);
    assert_eq!(page["jobs"][0], serde_json::to_value(job).unwrap());
    let keys: Vec<_> = page["jobs"][0]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        [
            "context",
            "created_at",
            "error_count",
            "finished_at",
            "id",
            "kind",
            "metrics",
            "phase",
            "state",
            "updated_at"
        ]
    );
    let metrics = page["jobs"][0]["metrics"].as_object().unwrap();
    assert_eq!(metrics.len(), 14);
    for key in [
        "queued_secs",
        "other_secs",
        "copy_secs",
        "source_check_secs",
        "destination_check_secs",
        "paused_secs",
        "decision_secs",
    ] {
        assert!(metrics[key].is_f64(), "{key} is not a JSON double");
    }
    for req in [
        json!({"space_id": null, "since": null, "pair_id": null, "offset": -1, "limit": 10}),
        json!({"space_id": null, "since": null, "pair_id": null, "offset": 0, "limit": -1}),
        json!({"space_id": null, "since": null, "pair_id": null, "offset": 0, "limit": 101}),
        json!({"space_id": null, "since": -1, "pair_id": null, "offset": 0, "limit": 10}),
    ] {
        assert!(ui
            .call("list_speed_analysis_jobs", json!({"req": req}))
            .is_err());
    }
    assert!(ui
        .call(
            "get_speed_analysis",
            json!({"req": {"space_id": null, "since": null, "pair_id": "invalid"}})
        )
        .is_err());
    let zero = ui.ok(
        "list_speed_analysis_jobs",
        json!({"req": {"space_id": null, "since": null, "pair_id": null, "offset": 0, "limit": 0}}),
    );
    assert_eq!(zero, json!({"jobs": [], "total": 1}));
}

#[test]
fn metadata_command_reports_missing_media_path() {
    let ui = Ui::start();
    let missing = ui._data.path().join("missing.jpg");
    let error = ui
        .call(
            "get_media_metadata",
            json!({ "absPath": missing.display().to_string() }),
        )
        .expect_err("missing media should return an explicit error");
    assert!(error.to_string().contains("I/O error"));
}

#[test]
fn opening_media_rejects_paths_not_listed_by_a_source() {
    let ui = Ui::start();
    let dir = tempfile::tempdir().unwrap();
    let image = dir.path().join("photo.jpg");
    std::fs::write(&image, b"image").unwrap();
    let error = ui
        .call(
            "open_media_file",
            json!({ "absPath": image.display().to_string() }),
        )
        .expect_err("unlisted local files must not be opened by the app");
    assert!(error
        .to_string()
        .contains("only available for files listed by a source"));
}

#[test]
fn project_free_workspace_ipc_copies_files_and_rejects_wipe_without_project() {
    let ui = Ui::start();
    let card = tempfile::tempdir().unwrap();
    let nas = tempfile::tempdir().unwrap();
    write(card.path(), "DCIM/A.JPG", b"project-free IPC photo");
    ui.save("space", json!({"id": "sp", "name": "Project-free"}));
    for (id, root, role) in [
        ("card", card.path(), "original"),
        ("nas", nas.path(), "final"),
    ] {
        ui.ok(
            "register_device",
            json!({
                "mountPath": root.display().to_string(),
                "device": {"id": id, "name": id, "kind": "other", "role": role}
            }),
        );
    }
    ui.save("source", json!({
        "id": "src", "space_id": "sp", "device_id": "card", "path_template": "DCIM", "offer_wipe": true
    }));
    ui.save(
        "destination",
        json!({
            "id": "dst", "space_id": "sp", "device_id": "nas",
            "path_template": "Photos", "subfolder_per_source": false, "use_backup_marker": false
        }),
    );
    ui.save(
        "flow",
        json!({"id": "f", "space_id": "sp", "source_id": "src", "destination_id": "dst"}),
    );
    let context = json!({"spaceId": "sp", "projectId": null});
    let pending = ui.ok("get_workspace_status", json!({"context": context}));
    assert_eq!(pending["context"], context);
    assert_eq!(pending["sources"][0]["file_count"], 1);
    assert!(pending["sources"][0]["required_copies"].is_null());
    assert_eq!(pending["flows"][0]["runnable"], true);
    let req = |category| {
        json!({
            "req": {"context": context, "flowId": "f", "category": category, "offset": 0, "limit": 50}
        })
    };
    let files = ui.ok("list_workspace_files", req("to_transfer"));
    assert_eq!(files["total"], 1);
    assert!(files["items"][0]["project_id"].is_null());
    assert_eq!(
        ui.ok("run_workspace_all", json!({"context": context}))
            .as_array()
            .unwrap()
            .len(),
        1
    );
    ui.wait_idle();
    let jobs = ui.ok("list_transfers", json!({}));
    assert!(
        jobs.as_array()
            .unwrap()
            .iter()
            .all(|j| j["state"] == "done"),
        "{jobs}"
    );
    assert_eq!(
        std::fs::read(nas.path().join("Photos/A.JPG")).unwrap(),
        b"project-free IPC photo"
    );
    assert_eq!(
        ui.ok("list_workspace_files", req("transferred"))["total"],
        1
    );
    let completed = ui.ok("get_workspace_status", json!({"context": context}));
    assert_eq!(completed["flows"][0]["state"], "done");
    assert_eq!(completed["sources"][0]["wipe_eligible"], false);
    for project_id in [Value::Null, json!("")] {
        assert!(ui
            .call(
                "wipe",
                json!({
                    "projectId": project_id, "sourceId": "src", "method": "delete_files"
                })
            )
            .is_err());
        let plan = ui.ok("plan_wipe", json!({ "sourceId": "src" }));
        assert_eq!(plan["eligible"], false);
    }
    assert!(card.path().join("DCIM/A.JPG").exists());
    assert!(ui.ok("get_snapshot", json!({}))["projects"]
        .as_array()
        .unwrap()
        .iter()
        .all(|p| p["space_id"] != "sp"));
    write(card.path(), "DCIM/B.JPG", b"second photo");
    ui.ok(
        "run_workspace_flow",
        json!({"context": context, "flowId": "f"}),
    );
    ui.wait_idle();
    assert_eq!(
        ui.ok("list_workspace_files", req("transferred"))["total"],
        2
    );
    assert_eq!(
        std::fs::read(nas.path().join("Photos/B.JPG")).unwrap(),
        b"second photo"
    );
    let error = ui
        .call(
            "confirm_workspace_app_import",
            json!({
                "context": context, "flowId": "f", "token": "invalid"
            }),
        )
        .unwrap_err();
    assert!(error.to_string().contains("no longer valid"));
}

/// A new user sets everything up from scratch with the UI's default factories.
#[test]
fn fresh_setup_backs_up_card_and_wipes_it() {
    let ui = Ui::start();
    let card = tempfile::tempdir().unwrap();
    let nas = tempfile::tempdir().unwrap();
    write(card.path(), "DCIM/100MSDCF/IMG_0001.ARW", b"raw-one");
    write(card.path(), "DCIM/100MSDCF/IMG_0002.JPG", b"jpeg-two");
    write(card.path(), "DCIM/100MSDCF/IMG_0002.THM", b"thumb");
    write(card.path(), "DCIM/100MSDCF/C0001.MP4", b"video");
    write(card.path(), "PRIVATE/M4ROOT/C0001.XML", b"xml");

    let snapshot = ui.ok("get_snapshot", json!({}));
    assert!(snapshot["computer"]["id"].is_string());

    // newSpace / newProject
    ui.save("space", json!({
        "id": "sp", "name": "Travel", "icon": "folder", "position": 9, "hash_algo": "blake3", "verify_mode": "inline",
        "variables": [{ "name": "project_name", "default_value": "", "required": true }],
        "backup_marker_template": "{date}_{project_name}",
    }));
    ui.save(
        "project",
        json!({
            "id": "pr", "space_id": "sp", "name": "Trip", "values": { "project_name": "Trip 2026" },
            "final_copies_required": 1, "archived": false,
        }),
    );
    let mut settings = snapshot["settings"].clone();
    settings["active_space_id"] = json!("sp");
    ui.ok("save_settings", json!({ "settings": settings }));

    // newDevice + registerDevice (writes a marker and maps the folder on this computer)
    let device = |id: &str, name: &str, kind: &str, role: &str| {
        json!({ "id": id, "name": name, "description": "", "role": role, "kind": kind,
                "hw_serial": null, "volume_uuid": null, "capacity_bytes": null })
    };
    let card_path = card.path().to_string_lossy();
    let nas_path = nas.path().to_string_lossy();
    ui.ok("register_device", json!({ "mountPath": card_path, "device": device("card", "Camera A Card 1", "sd_card", "original") }));
    ui.ok(
        "register_device",
        json!({ "mountPath": nas_path, "device": device("nas", "Home NAS", "nas", "final") }),
    );
    assert!(card.path().join(".openmediabackup/device.json").exists());

    // Timestamp-free fixtures use a project-independent destination.
    ui.save("source", json!({ "id": "src", "space_id": "sp", "device_id": "card", "path_template": "", "offer_wipe": true, "position": 0, "project_scope": { "mode": "none" } }));
    ui.save(
        "destination",
        json!({
            "id": "dst", "space_id": "sp", "device_id": "nas", "path_template": "Trip 2026",
            "subfolder_per_source": true, "counts_as_safe_copy": true, "use_backup_marker": false,
            "rules": [
                { "action": "exclude", "syntax": "glob", "pattern": "PRIVATE/" },
                { "action": "exclude", "syntax": "glob", "pattern": "*.THM" },
                { "action": "exclude", "syntax": "glob", "pattern": "*.MP4" },
                { "action": "exclude", "syntax": "glob", "pattern": ".*" },
            ],
            "position": 0,
        }),
    );
    ui.save(
        "flow",
        json!({ "id": "fl", "space_id": "sp", "source_id": "src", "destination_id": "dst" }),
    );

    let flow = ui.flow_status("pr");
    assert_eq!(flow["state"], "pending", "{flow}");
    assert_eq!(flow["to_transfer"], 2, "{flow}");
    assert_eq!(flow["ignored"], 3, "{flow}");

    let page = ui.ok("list_files", json!({ "req": {
        "projectId": "pr", "flowId": "fl", "category": "to_transfer", "offset": 0, "limit": 50, "filter": null } }));
    assert_eq!(page["total"], 2);
    ui.ok("run_flow", json!({ "projectId": "pr", "flowId": "fl" }));
    ui.wait_idle();

    let flow = ui.flow_status("pr");
    assert_eq!(
        (flow["state"].as_str(), flow["transferred"].as_u64()),
        (Some("done"), Some(2)),
        "{flow}"
    );
    let copied: Vec<_> = walkdir::WalkDir::new(nas.path())
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| {
            e.file_type().is_file() && !e.path().to_string_lossy().contains(".openmediabackup")
        })
        .map(|e| {
            e.path()
                .strip_prefix(nas.path())
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect();
    assert_eq!(copied.len(), 2, "{copied:?}");
    assert!(
        copied
            .iter()
            .all(|p| p.contains("Trip 2026/Camera A Card 1/")),
        "{copied:?}"
    );

    let plan = ui.ok("plan_wipe", json!({ "sourceId": "src" }));
    assert_eq!(
        (
            plan["files_total"].as_u64(),
            plan["ignored"].as_u64(),
            plan["eligible"].as_bool()
        ),
        (Some(5), Some(3), Some(true)),
        "{plan}"
    );
    ui.ok(
        "wipe",
        json!({ "sourceId": "src", "method": "delete_files" }),
    );
    ui.wait_idle();
    assert!(!card.path().join("DCIM/100MSDCF/IMG_0001.ARW").exists());
    assert!(
        card.path().join("PRIVATE/M4ROOT/C0001.XML").exists(),
        "ignored files are kept"
    );
    assert!(
        card.path().join("DCIM/100MSDCF/C0001.MP4").exists(),
        "ignored videos are kept"
    );
}

#[test]
fn corrupted_copy_is_flagged_and_card_not_wipeable() {
    let ui = Ui::start();
    let card = tempfile::tempdir().unwrap();
    let nas = tempfile::tempdir().unwrap();
    write(card.path(), "DCIM/A.JPG", b"original");
    let device = |id: &str, kind: &str, role: &str| {
        json!({ "id": id, "name": id, "description": "", "role": role, "kind": kind,
                "hw_serial": null, "volume_uuid": null, "capacity_bytes": null })
    };
    ui.ok("register_device", json!({ "mountPath": card.path().to_string_lossy(), "device": device("card", "sd_card", "original") }));
    ui.ok("register_device", json!({ "mountPath": nas.path().to_string_lossy(), "device": device("nas", "nas", "final") }));
    ui.save(
        "space",
        json!({ "id": "sp", "name": "S", "icon": "folder", "position": 0, "hash_algo": "blake3",
        "verify_mode": "reread", "variables": [], "backup_marker_template": "" }),
    );
    ui.save("project", json!({ "id": "pr", "space_id": "sp", "name": "P", "values": {}, "final_copies_required": 1, "archived": false }));
    ui.save("source", json!({ "id": "src", "space_id": "sp", "device_id": "card", "path_template": "", "offer_wipe": true, "position": 0 }));
    ui.save("destination", json!({ "id": "dst", "space_id": "sp", "device_id": "nas", "path_template": "backup",
        "subfolder_per_source": false, "counts_as_safe_copy": true, "use_backup_marker": false, "rules": [], "position": 0 }));
    ui.save(
        "flow",
        json!({ "id": "fl", "space_id": "sp", "source_id": "src", "destination_id": "dst" }),
    );

    ui.ok("run_all", json!({ "projectId": "pr" }));
    ui.wait_idle();
    assert_eq!(ui.flow_status("pr")["state"], "done");

    // The card file changes after the backup: wiping must refuse instead of deleting unverified data.
    std::fs::write(card.path().join("DCIM/A.JPG"), b"changed!").unwrap();
    ui.ok(
        "wipe",
        json!({ "projectId": "pr", "sourceId": "src", "method": "delete_files" }),
    );
    ui.wait_idle();
    assert!(card.path().join("DCIM/A.JPG").exists());
}

/// Thumbnails come back as raw JPEG bytes over IPC (what `tauri-backend.ts` turns into a blob URL).
#[test]
fn thumbnail_returns_jpeg_bytes_over_ipc() {
    let ui = Ui::start();
    let dir = tempfile::tempdir().unwrap();
    let photo = dir.path().join("IMG_0001.JPG");
    image::RgbImage::from_pixel(1200, 800, image::Rgb([200, 40, 40]))
        .save_with_format(&photo, image::ImageFormat::Jpeg)
        .unwrap();
    let notes = dir.path().join("notes.txt");
    std::fs::write(&notes, b"hi").unwrap();

    let bytes = |path: &std::path::Path| -> Vec<u8> {
        let body = get_ipc_response(
            &ui.webview,
            ui.request("thumbnail", json!({ "absPath": path })),
        )
        .unwrap_or_else(|e| panic!("thumbnail failed: {e}"));
        match body {
            InvokeResponseBody::Raw(b) => b.to_vec(),
            other => panic!("expected raw bytes, got {other:?}"),
        }
    };
    let thumb = bytes(&photo);
    assert_eq!(&thumb[..2], &[0xff, 0xd8], "JPEG magic");
    let decoded = image::load_from_memory(&thumb).unwrap();
    assert_eq!((decoded.width(), decoded.height()), (360, 240));
    assert!(bytes(&notes).is_empty());
}

#[test]
fn destination_check_and_interactive_conflicts_over_ipc() {
    let ui = Ui::start();
    let card = tempfile::tempdir().unwrap();
    let nas = tempfile::tempdir().unwrap();
    write(card.path(), "DCIM/A.JPG", b"card photo");
    write(card.path(), "DCIM/B.JPG", b"second");
    write(nas.path(), "Photos/A.JPG", b"other");
    ui.save("space", json!({"id": "sp", "name": "Workspace"}));
    for (id, root, role) in [
        ("card", card.path(), "original"),
        ("nas", nas.path(), "final"),
    ] {
        ui.ok(
            "register_device",
            json!({
                "mountPath": root.display().to_string(),
                "device": {"id": id, "name": id, "kind": "other", "role": role}
            }),
        );
    }
    ui.save("source", json!({
        "id": "src", "space_id": "sp", "device_id": "card", "path_template": "DCIM", "offer_wipe": false
    }));
    ui.save(
        "destination",
        json!({
            "id": "dst", "space_id": "sp", "device_id": "nas",
            "path_template": "Photos", "subfolder_per_source": false, "use_backup_marker": false
        }),
    );
    ui.save(
        "flow",
        json!({"id": "f", "space_id": "sp", "source_id": "src", "destination_id": "dst"}),
    );
    let context = json!({"spaceId": "sp", "projectId": null});
    let args = json!({"context": context, "destinationId": "dst"});

    let checks = ui.ok("check_workspace_destination", args.clone());
    assert_eq!(checks.as_array().unwrap().len(), 1);
    ui.wait_idle();
    let jobs = ui.ok("list_transfers", json!({}));
    let check = &jobs[0];
    assert_eq!(check["kind"], "check");
    assert_eq!(check["state"], "done");
    assert_eq!(check["check_results"]["missing"], 1);
    assert_eq!(check["check_results"]["conflicts"], 1);
    assert_eq!(check["check_results"]["matched"], 0);
    assert_eq!(check["check_results"]["items"][0]["outcome"], "conflict");
    assert!(check["pending_conflict"].is_null());
    assert!(!nas.path().join("Photos/B.JPG").exists());
    assert_eq!(
        std::fs::read(nas.path().join("Photos/A.JPG")).unwrap(),
        b"other"
    );

    let runs = ui.ok("run_workspace_destination", args);
    assert_eq!(runs.as_array().unwrap().len(), 1);
    let job_id = runs[0].as_str().unwrap().to_string();
    let deadline = Instant::now() + Duration::from_secs(10);
    let pending = loop {
        let jobs = ui.ok("list_transfers", json!({}));
        let job = jobs
            .as_array()
            .unwrap()
            .iter()
            .find(|j| j["id"] == job_id.as_str())
            .unwrap()
            .clone();
        if job["state"] == "awaiting_decision" {
            break job["pending_conflict"].clone();
        }
        assert!(Instant::now() < deadline, "no conflict request: {job}");
        std::thread::sleep(Duration::from_millis(30));
    };
    assert!(pending["request_id"].as_str().is_some());
    assert!(pending["destination_path"]
        .as_str()
        .unwrap()
        .ends_with("A.JPG"));
    let resolve = |request: &str, apply: bool| {
        ui.call(
            "resolve_transfer_conflict",
            json!({
                "jobId": job_id, "requestId": request,
                "decision": "replace", "applyToRemaining": apply
            }),
        )
    };
    assert!(resolve("stale-request", true).is_err());
    resolve(pending["request_id"].as_str().unwrap(), false).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while ui
        .ok("list_transfers", json!({}))
        .as_array()
        .unwrap()
        .iter()
        .any(|j| j["id"] == job_id.as_str() && j["state"] != "done")
    {
        assert!(Instant::now() < deadline, "job did not finish");
        std::thread::sleep(Duration::from_millis(30));
    }
    assert_eq!(
        std::fs::read(nas.path().join("Photos/A.JPG")).unwrap(),
        b"card photo"
    );
    assert_eq!(
        std::fs::read(nas.path().join("Photos/B.JPG")).unwrap(),
        b"second"
    );
    assert!(resolve(pending["request_id"].as_str().unwrap(), false).is_err());
    assert!(ui
        .call(
            "check_workspace_destination",
            json!({"context": {"spaceId": "sp", "projectId": null}, "destinationId": "missing"})
        )
        .is_err());
}
