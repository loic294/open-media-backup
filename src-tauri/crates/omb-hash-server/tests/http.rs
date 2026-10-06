use omb_hash::{hash_file, HashAlgo};
use omb_hash_server::{router, Browse, HashRequest, HashResponse, HashServer, Hello, Root};
use reqwest::{Client, StatusCode};

#[tokio::test]
async fn authenticated_endpoints_parity_and_path_safety() {
    let data = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(data.path().join("image"), b"abc").unwrap();
    std::fs::create_dir(data.path().join("nested")).unwrap();
    std::fs::write(outside.path().join("secret"), b"private").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(outside.path(), data.path().join("escape")).unwrap();
    let server = HashServer::new(
        "id".into(),
        "NAS".into(),
        "token".into(),
        vec![data.path().into()],
        1,
    )
    .unwrap();
    let root = server.roots()[0].id.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move { axum::serve(listener, router(server)).await.unwrap() });
    let client = Client::new();
    for path in ["hello", "roots", "roots/bad/browse"] {
        assert_eq!(
            client
                .get(format!("{url}/v1/{path}"))
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::UNAUTHORIZED
        );
    }
    assert_eq!(
        client
            .post(format!("{url}/v1/hash"))
            .bearer_auth("tokem")
            .json(&HashRequest {
                root: root.clone(),
                rel_path: "image".into(),
                algo: HashAlgo::Blake3,
            })
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let hello: Hello = client
        .get(format!("{url}/v1/hello"))
        .bearer_auth("token")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(hello.kind, "hash_server");
    let roots: Vec<Root> = client
        .get(format!("{url}/v1/roots"))
        .bearer_auth("token")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(roots.len(), 1);
    let browse: Browse = client
        .get(format!("{url}/v1/roots/{root}/browse"))
        .bearer_auth("token")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(browse.directories, vec!["nested"]);
    for algo in [HashAlgo::Xxh64, HashAlgo::Blake3] {
        let response: HashResponse = client
            .post(format!("{url}/v1/hash"))
            .bearer_auth("token")
            .json(&HashRequest {
                root: root.clone(),
                rel_path: "image".into(),
                algo,
            })
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(
            response.hash,
            hash_file(&data.path().join("image"), algo, |_| true).unwrap()
        );
        assert_eq!(response.size, 3);
        assert!(response.modified.is_some());
    }
    for path in [
        "../secret",
        "nested/../../secret",
        "/etc/passwd",
        "C:\\secret",
        "\\\\server\\share",
    ] {
        assert_eq!(
            client
                .post(format!("{url}/v1/hash"))
                .bearer_auth("token")
                .json(&HashRequest {
                    root: root.clone(),
                    rel_path: path.into(),
                    algo: HashAlgo::Blake3,
                })
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN,
            "{path}"
        );
    }
    #[cfg(unix)]
    assert_eq!(
        client
            .post(format!("{url}/v1/hash"))
            .bearer_auth("token")
            .json(&HashRequest {
                root: root.clone(),
                rel_path: "escape/secret".into(),
                algo: HashAlgo::Blake3,
            })
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        client
            .post(format!("{url}/v1/hash"))
            .bearer_auth("token")
            .json(&HashRequest {
                root,
                rel_path: "missing".into(),
                algo: HashAlgo::Blake3,
            })
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    task.abort();
    let _ = task.await;
}
