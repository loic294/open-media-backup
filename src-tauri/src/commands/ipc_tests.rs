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
    settings["active_project_by_space"] = json!({ "sp": "pr" });
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

    let plan = ui.ok("plan_wipe", json!({ "projectId": "pr", "sourceId": "src" }));
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
        json!({ "projectId": "pr", "sourceId": "src", "method": "delete_files" }),
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
