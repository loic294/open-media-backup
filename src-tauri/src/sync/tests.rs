use super::*;
use crate::{domain::Space, store::Store};
use std::sync::Arc;

fn space(id: &str, name: &str) -> Space {
    Space {
        id: id.into(),
        name: name.into(),
        ..Default::default()
    }
}

#[tokio::test]
async fn real_server_syncs_and_rejects_wrong_token() {
    let a = Arc::new(Store::open_in_memory().unwrap());
    let b = Arc::new(Store::open_in_memory().unwrap());
    let mut synced_space = space("a", "from-a");
    synced_space.skip_counts_as_safe_copy = true;
    a.put(&synced_space).unwrap();
    b.put(&space("b", "from-b")).unwrap();

    let service_a = SyncService::new(a.clone(), |_| {});
    let service_b = SyncService::new(b.clone(), |_| {});
    let token_b = service_b.token();
    let addr = service_b.start_server(0).await.unwrap();
    let address = format!("127.0.0.1:{}", addr.port());

    let wrong = reqwest::Client::new()
        .get(format!("http://{address}/v1/hello"))
        .bearer_auth("wrong")
        .send()
        .await
        .unwrap();
    assert_eq!(wrong.status(), reqwest::StatusCode::UNAUTHORIZED);

    service_a.add_peer(&address, &token_b).await.unwrap();
    service_a.sync_now().await.unwrap();
    assert_eq!(a.version_vector().unwrap(), b.version_vector().unwrap());
    assert!(a.get::<Space>("b").unwrap().is_some());
    assert!(b.get::<Space>("a").unwrap().is_some());
    assert!(
        b.get::<Space>("a")
            .unwrap()
            .unwrap()
            .skip_counts_as_safe_copy
    );
}
