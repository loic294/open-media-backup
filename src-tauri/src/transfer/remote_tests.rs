use super::{copy::copy_resolving_with_hasher, destination_hasher::DestinationHasher, *};
use crate::{
    domain::{HashAlgo, RemoteHash, VerifyMode},
    hash_server::{Client, HashRequest, HashResponse},
    plan::{resolve_flow, FailureMap, WorkspaceContext},
    store::HashServer,
    testing::{Fixture, HttpServer as Server},
};
use axum::{response::IntoResponse, routing::post, Json, Router};
use parking_lot::Mutex;
use std::{
    path::Path,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

fn paired(fx: &Fixture) -> (Server, String) {
    let server = omb_hash_server::HashServer::new(
        crate::domain::new_id(),
        "NAS".into(),
        "token".into(),
        vec![fx.nas_dir.path().into()],
        2,
    )
    .unwrap();
    let root = server.roots()[0].id.clone();
    let server = Server::start(omb_hash_server::router(server));
    map(fx, &server.address, &root);
    (server, root)
}

fn map(fx: &Fixture, address: &str, root: &str) {
    fx.store
        .save_hash_server(&HashServer {
            id: "nas-server".into(),
            name: "NAS".into(),
            address: address.into(),
            token: "token".into(),
            ..Default::default()
        })
        .unwrap();
    let mut destination = fx.destination.clone();
    destination.remote_hash = Some(RemoteHash {
        server_id: "nas-server".into(),
        root: root.into(),
        enabled: true,
    });
    fx.store.put(&destination).unwrap();
}

fn handle(fx: &Fixture, kind: JobKind) -> JobHandle {
    let ctx = resolve_flow(&fx.store, &fx.resolver, &fx.project.id, &fx.flow.id).unwrap();
    let handle = JobHandle::new(
        TransferJob::new(
            crate::domain::new_id(),
            fx.flow.id.clone(),
            String::new(),
            kind,
        ),
        Arc::new(|| {}),
    )
    .with_analysis(Some(AnalysisContext::from_flow(&ctx)), None);
    handle.update(|j| j.state = JobState::Running);
    handle
}

fn transfer(fx: &Fixture) -> TransferJob {
    let h = handle(fx, JobKind::Transfer);
    run_transfer(
        &fx.store,
        &fx.resolver,
        &fx.project.id,
        &fx.flow.id,
        &h,
        &Mutex::new(FailureMap::new()),
    )
    .unwrap();
    h.snapshot()
}

#[test]
fn reread_and_check_destination_use_server_without_network_reads() {
    let fx = Fixture::new();
    fx.write_card_file("DCIM/A.JPG", b"photo");
    let (_server, _root) = paired(&fx);
    let job = transfer(&fx);
    assert!(job.errors.is_empty(), "{:?}", job.errors);
    assert!(job.warnings.is_empty());
    assert!(job.remote_hash_active);
    let metrics = job.analysis.unwrap().metrics;
    assert_eq!(metrics.destination_check_bytes, 0);
    assert_eq!(metrics.remote_check_bytes, 5);
    let h = handle(&fx, JobKind::Check);
    run_workspace_check(
        &fx.store,
        &fx.resolver,
        &WorkspaceContext {
            space_id: fx.space.id.clone(),
            project_id: Some(fx.project.id.clone()),
        },
        &fx.flow.id,
        &h,
    )
    .unwrap();
    let checked = h.snapshot();
    assert_eq!(checked.check_results.unwrap().matched, 1);
    assert_eq!(checked.bytes_done, 5);
    let metrics = checked.analysis.unwrap().metrics;
    assert_eq!(metrics.destination_check_bytes, 0);
    assert_eq!(metrics.source_check_bytes, 5);
    assert_eq!(metrics.remote_check_bytes, 5);
}

fn remote(fx: &Fixture, server: &Server, root: String) -> DestinationHasher {
    DestinationHasher::Remote {
        client: Client::new(&HashServer {
            address: server.address.clone(),
            token: "token".into(),
            ..Default::default()
        })
        .unwrap(),
        root,
        dest_root: fx.nas_dir.path().into(),
    }
}

#[test]
fn conflict_detection_stage_and_replace_recheck_use_remote_for_both_algorithms() {
    for algo in [HashAlgo::Blake3, HashAlgo::Xxh64] {
        let fx = Fixture::new();
        let src = fx.write_card_file("DCIM/A.JPG", b"new");
        let dst = fx.nas_dir.path().join("A.JPG");
        std::fs::write(&dst, b"old").unwrap();
        let (server, root) = paired(&fx);
        let hasher = remote(&fx, &server, root);
        let h = handle(&fx, JobKind::Transfer);
        let result = copy_resolving_with_hasher(
            &src,
            &dst,
            algo,
            VerifyMode::Reread,
            None,
            &h,
            &hasher,
            &mut |info| {
                assert_ne!(info.source_hash, info.destination_hash);
                Ok(ConflictDecision::Replace)
            },
        )
        .unwrap();
        assert!(result.replaced);
        assert_eq!(std::fs::read(&dst).unwrap(), b"new");
        let metrics = h.snapshot().analysis.unwrap().metrics;
        assert_eq!(metrics.destination_check_bytes, 0);
        assert_eq!(metrics.remote_check_bytes, 9);
        let adopted = copy_resolving_with_hasher(
            &src,
            &dst,
            algo,
            VerifyMode::Inline,
            None,
            &h,
            &hasher,
            &mut |_| panic!("identical existing file must be adopted"),
        )
        .unwrap();
        assert!(adopted.adopted);
    }
}

#[test]
fn unreachable_server_and_missing_remote_file_fall_back_with_warning() {
    for missing in [false, true] {
        let fx = Fixture::new();
        fx.write_card_file("DCIM/A.JPG", b"photo");
        let (server, root) = paired(&fx);
        if missing {
            map(&fx, &server.address, &format!("{root}/wrong-folder"));
        } else {
            drop(server);
        }
        let job = transfer(&fx);
        assert!(job.errors.is_empty());
        assert_eq!(job.warnings.len(), 1);
        assert!(job.warnings[0].contains("fell back to local re-read"));
        let metrics = job.analysis.unwrap().metrics;
        assert_eq!(metrics.destination_check_bytes, 5);
        assert_eq!(metrics.remote_check_bytes, 0);
    }
}

#[test]
fn size_mismatch_and_invalid_digest_never_pass_without_local_reread() {
    for response in [
        HashResponse {
            hash: "0".repeat(64),
            size: 99,
            modified: None,
        },
        HashResponse {
            hash: "not-a-hash".into(),
            size: 5,
            modified: None,
        },
    ] {
        let router = Router::new().route(
            "/v1/hash",
            post(move || {
                let response = response.clone();
                async move { Json(response) }
            }),
        );
        let server = Server::start(router);
        let fx = Fixture::new();
        fx.write_card_file("DCIM/A.JPG", b"photo");
        map(&fx, &server.address, &"a".repeat(64));
        let job = transfer(&fx);
        assert!(job.errors.is_empty());
        assert_eq!(job.warnings.len(), 1);
        let metrics = job.analysis.unwrap().metrics;
        assert_eq!(metrics.destination_check_bytes, 5);
        assert_eq!(metrics.remote_check_bytes, 0);
    }
}

#[test]
fn inline_copy_does_not_request_remote_verification_and_unknown_server_is_local() {
    let fx = Fixture::new();
    fx.write_card_file("DCIM/A.JPG", b"photo");
    let (_server, _root) = paired(&fx);
    let mut space = fx.space.clone();
    space.verify_mode = VerifyMode::Inline;
    fx.store.put(&space).unwrap();
    let job = transfer(&fx);
    assert!(job.warnings.is_empty());
    let metrics = job.analysis.unwrap().metrics;
    assert_eq!(metrics.remote_check_bytes, 0);
    assert_eq!(metrics.destination_check_bytes, 0);
    let unknown = DestinationHasher::resolve(
        &fx.store,
        Some(&RemoteHash {
            server_id: "another-computer".into(),
            root: "root".into(),
            enabled: true,
        }),
        fx.nas_dir.path(),
    )
    .unwrap();
    assert!(!unknown.is_remote());
    let h = handle(&fx, JobKind::Check);
    let path = fx.nas_dir.path().join("test");
    std::fs::write(&path, b"test").unwrap();
    assert_eq!(
        unknown
            .hash(&path, HashAlgo::Blake3, &h, |_, _| {})
            .unwrap(),
        crate::hashing::hash_file(&path, HashAlgo::Blake3, |_| true).unwrap()
    );
    assert_eq!(h.snapshot().warnings.len(), 1);
}

#[test]
fn cancellation_drops_pending_http_request_and_never_records_verified_bytes() {
    let (send, receive) = std::sync::mpsc::channel();
    let server = Server::start(Router::new().route(
        "/v1/hash",
        post(move |Json(_): Json<HashRequest>| {
            send.send(()).unwrap();
            async {
                tokio::time::sleep(Duration::from_millis(500)).await;
                Json(HashResponse {
                    hash: "0".repeat(64),
                    size: 5,
                    modified: None,
                })
            }
        }),
    ));
    let fx = Fixture::new();
    let file = fx.nas_dir.path().join("test");
    std::fs::write(&file, b"photo").unwrap();
    let hasher = remote(&fx, &server, "a".repeat(64));
    let h = Arc::new(handle(&fx, JobKind::Check));
    let worker = h.clone();
    let thread = thread::spawn(move || hasher.hash(&file, HashAlgo::Blake3, &worker, |_, _| {}));
    receive.recv_timeout(Duration::from_secs(2)).unwrap();
    let start = Instant::now();
    h.cancel();
    assert!(matches!(thread.join().unwrap(), Err(CopyError::Cancelled)));
    assert!(start.elapsed() < Duration::from_millis(400));
    let job = h.snapshot();
    assert!(job.warnings.is_empty());
    assert_eq!(job.analysis.unwrap().metrics.remote_check_bytes, 0);
}

#[test]
fn remote_target_outside_device_root_is_rejected_not_sent() {
    let fx = Fixture::new();
    let (server, root) = paired(&fx);
    let h = handle(&fx, JobKind::Check);
    let file = fx.write_card_file("outside", b"photo");
    assert!(remote(&fx, &server, root)
        .hash(Path::new(&file), HashAlgo::Blake3, &h, |_, _| {})
        .is_err());
    assert_eq!(h.snapshot().analysis.unwrap().metrics.remote_check_bytes, 0);
}

fn destination_check(fx: &Fixture) -> TransferJob {
    let h = handle(fx, JobKind::Check);
    let destination = fx.store.get(&fx.destination.id).unwrap().unwrap();
    super::run_destination_check(
        &fx.store,
        &fx.resolver,
        &WorkspaceContext {
            space_id: fx.space.id.clone(),
            project_id: Some(fx.project.id.clone()),
        },
        &destination,
        &h,
    )
    .unwrap();
    h.snapshot()
}

fn copied_paths(fx: &Fixture) -> Vec<String> {
    fx.store
        .list::<crate::domain::FileCopy>()
        .unwrap()
        .into_iter()
        .filter(|copy| copy.device_id == fx.destination.device_id && !copy.removed)
        .map(|copy| copy.path)
        .collect()
}

#[test]
fn full_destination_check_lists_and_hashes_on_the_server() {
    let fx = Fixture::new();
    fx.write_card_file("DCIM/A.JPG", b"photo");
    fx.write_card_file("DCIM/B.JPG", b"other");
    let server = omb_hash_server::HashServer::new(
        crate::domain::new_id(),
        "NAS".into(),
        "token".into(),
        vec![fx.nas_dir.path().into()],
        2,
    )
    .unwrap();
    let root = server.roots()[0].id.clone();
    let lists = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter = lists.clone();
    let app = omb_hash_server::router(server).layer(axum::middleware::from_fn(
        move |req: axum::extract::Request, next: axum::middleware::Next| {
            if req.uri().path().ends_with("/list") {
                counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
            next.run(req)
        },
    ));
    let server = Server::start(app);
    map(&fx, &server.address, &root);
    assert!(transfer(&fx).errors.is_empty());
    let copied = copied_paths(&fx);
    assert_eq!(copied.len(), 2);
    let removed = copied.iter().find(|p| p.ends_with("B.JPG")).unwrap();
    std::fs::remove_file(fx.nas_dir.path().join(removed)).unwrap();
    let kept = copied.iter().find(|p| p.ends_with("A.JPG")).unwrap();
    let folder = Path::new(kept).parent().unwrap();
    std::fs::write(fx.nas_dir.path().join(folder).join("extra.bin"), b"x").unwrap();

    let job = destination_check(&fx);
    assert!(job.warnings.is_empty(), "{:?}", job.warnings);
    assert!(job.remote_hash_active);
    assert!(lists.load(std::sync::atomic::Ordering::SeqCst) > 0);
    let results = job.check_results.unwrap();
    assert_eq!(
        (
            results.verified,
            results.untracked,
            results.missing,
            results.errors
        ),
        (1, 1, 1, 0),
        "{:?}",
        results.items
    );
    assert_eq!(job.files_done, job.files_total);
    let metrics = job.analysis.unwrap().metrics;
    assert_eq!(metrics.destination_check_bytes, 0);
    assert_eq!(metrics.remote_check_bytes, 5);
}

#[test]
fn full_destination_check_detects_remote_changes_and_supports_old_servers() {
    let fx = Fixture::new();
    fx.write_card_file("DCIM/A.JPG", b"photo");
    let (_server, _root) = paired(&fx);
    assert!(transfer(&fx).errors.is_empty());
    let copied = copied_paths(&fx);
    std::fs::write(fx.nas_dir.path().join(&copied[0]), b"PHOTO").unwrap();
    let results = destination_check(&fx).check_results.unwrap();
    assert_eq!(results.conflicts, 1, "{:?}", results.items);

    // A server without the listing endpoint falls back to a local walk.
    let fx = Fixture::new();
    fx.write_card_file("DCIM/A.JPG", b"photo");
    let (_server, root) = paired(&fx);
    assert!(transfer(&fx).errors.is_empty());
    let server = omb_hash_server::HashServer::new(
        crate::domain::new_id(),
        "NAS".into(),
        "token".into(),
        vec![fx.nas_dir.path().into()],
        2,
    )
    .unwrap();
    let old = omb_hash_server::router(server).layer(axum::middleware::from_fn(
        |req: axum::extract::Request, next: axum::middleware::Next| async move {
            if req.uri().path().ends_with("/list") {
                return axum::http::StatusCode::NOT_FOUND.into_response();
            }
            next.run(req).await
        },
    ));
    let old = Server::start(old);
    map(&fx, &old.address, &root);
    let job = destination_check(&fx);
    let results = job.check_results.unwrap();
    assert_eq!(results.verified, 1, "{:?}", results.items);
    assert!(job.warnings.is_empty(), "{:?}", job.warnings);
}
