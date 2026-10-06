use super::service;
use crate::{
    domain::{Destination, FileCopy, RemoteHash},
    testing::{Fixture, HttpServer},
};

fn paired(fx: &Fixture, path: &std::path::Path) -> HttpServer {
    let id = crate::domain::new_id();
    let server = omb_hash_server::HashServer::new(
        id.clone(),
        "NAS".into(),
        "token".into(),
        vec![path.into()],
        2,
    )
    .unwrap();
    let root = server.roots()[0].id.clone();
    let server = HttpServer::start(omb_hash_server::router(server));
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime
        .block_on(service::add(&fx.store, &server.address, "token"))
        .unwrap();
    let mut destination = fx.destination.clone();
    destination.remote_hash = Some(RemoteHash {
        server_id: id,
        root,
        enabled: true,
    });
    fx.store.put(&destination).unwrap();
    server
}

fn known_file(fx: &Fixture, path: &str, bytes: &[u8]) {
    std::fs::write(fx.nas_dir.path().join(path), bytes).unwrap();
    fx.store
        .put(&FileCopy {
            id: FileCopy::id_for("file", &fx.nas.id, path),
            file_id: "file".into(),
            device_id: fx.nas.id.clone(),
            path: path.into(),
            ..Default::default()
        })
        .unwrap();
}

#[test]
fn registration_and_mapping_require_evidence_and_record_status_without_credentials() {
    let fx = Fixture::new();
    let server = paired(&fx, fx.nas_dir.path());
    let empty = service::test_mapping(&fx.store, &fx.resolver, &fx.destination.id).unwrap();
    assert!(!empty.verified);
    assert!(empty.message.contains("does not prove"));
    known_file(&fx, "image.jpg", b"same NAS file");
    let result = service::test_mapping(&fx.store, &fx.resolver, &fx.destination.id).unwrap();
    assert!(result.verified);
    let servers = fx.store.hash_servers().unwrap();
    assert!(servers[0].last_seen.is_some());
    assert!(servers[0].last_error.is_none());
    let json = serde_json::to_value(&servers).unwrap();
    assert!(json[0].get("token").is_none());
    drop(server);
    assert!(service::test_mapping(&fx.store, &fx.resolver, &fx.destination.id).is_err());
    assert!(fx.store.hash_servers().unwrap()[0].last_error.is_some());
}

#[test]
fn mapping_rejects_different_content_and_missing_remote_file() {
    let fx = Fixture::new();
    let remote = tempfile::tempdir().unwrap();
    let _server = paired(&fx, remote.path());
    known_file(&fx, "image.jpg", b"123456");
    std::fs::write(remote.path().join("image.jpg"), b"abcdef").unwrap();
    assert!(
        service::test_mapping(&fx.store, &fx.resolver, &fx.destination.id)
            .unwrap_err()
            .contains("hashes differ")
    );
    std::fs::remove_file(remote.path().join("image.jpg")).unwrap();
    assert!(
        service::test_mapping(&fx.store, &fx.resolver, &fx.destination.id)
            .unwrap_err()
            .contains("404")
    );
}

#[cfg(unix)]
#[test]
fn mapping_does_not_read_known_symlink_outside_local_device_root() {
    let fx = Fixture::new();
    let _server = paired(&fx, fx.nas_dir.path());
    known_file(&fx, "image.jpg", b"placeholder");
    let outside = tempfile::NamedTempFile::new().unwrap();
    std::fs::remove_file(fx.nas_dir.path().join("image.jpg")).unwrap();
    std::os::unix::fs::symlink(outside.path(), fx.nas_dir.path().join("image.jpg")).unwrap();
    assert!(
        service::test_mapping(&fx.store, &fx.resolver, &fx.destination.id)
            .unwrap_err()
            .contains("escapes destination device root")
    );
}

#[test]
fn credentials_and_invalid_addresses_cannot_be_registered() {
    let fx = Fixture::new();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    for address in [
        "https://127.0.0.1:47822",
        "http://user:password@127.0.0.1:47822",
        "127.0.0.1:47822?secret=1",
    ] {
        assert!(runtime
            .block_on(service::add(&fx.store, address, "token"))
            .is_err());
    }
    assert!(runtime
        .block_on(service::add(&fx.store, "127.0.0.1:47822", ""))
        .is_err());
    assert!(fx.store.hash_servers().unwrap().is_empty());
    assert!(service::test_mapping(&fx.store, &fx.resolver, &fx.destination.id).is_err());
    let destination: Destination = fx.store.get(&fx.destination.id).unwrap().unwrap();
    assert!(destination.remote_hash.is_none());
}
