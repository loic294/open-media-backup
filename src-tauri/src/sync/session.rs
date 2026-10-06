use crate::{
    store::{Op, Peer, Store, VersionVector},
    sync::{SyncError, SyncResult, SyncTransport, PAGE_SIZE},
};
use std::{sync::Arc, time::Instant};

#[derive(Debug, Clone, Default)]
pub struct SyncRound {
    pub pulled: usize,
    pub pushed: usize,
    pub latency_ms: u64,
}

pub async fn sync_peer(
    store: Arc<Store>,
    transport: &dyn SyncTransport,
    peer: &Peer,
) -> SyncResult<SyncRound> {
    sync_peer_with_page_size(store, transport, peer, PAGE_SIZE, |_| {}).await
}

pub async fn sync_peer_with_page_size(
    store: Arc<Store>,
    transport: &dyn SyncTransport,
    peer: &Peer,
    page_size: usize,
    mut on_progress: impl FnMut(f32) + Send,
) -> SyncResult<SyncRound> {
    let started = Instant::now();
    let hello = transport.hello(peer).await?;
    let latency_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    if hello.computer_id == store.computer_id() {
        return Err(SyncError::InvalidPeer(
            "cannot sync with this computer".into(),
        ));
    }
    on_progress(0.1);

    let mut pulled = 0;
    let mut peer_vv = loop {
        let local_vv = store.version_vector()?;
        let response = transport.pull(peer, local_vv).await?;
        let more = response.more;
        if !response.ops.is_empty() {
            store.apply_remote(&response.ops)?;
            pulled += response.ops.len();
        }
        on_progress(if more { 0.35 } else { 0.6 });
        if !more {
            break response.version_vector;
        }
    };

    // Legacy peers may send spaces before the projects that define their copy
    // policies. Migrate only after the complete pull so pagination cannot lock
    // in the new-space default before older project requirements arrive.
    store.migrate_copy_policy()?;

    let mut pushed = 0;
    loop {
        let ops = store.ops_since(&peer_vv, page_size)?;
        if ops.is_empty() {
            break;
        }
        transport.push(peer, ops.clone()).await?;
        pushed += ops.len();
        advance_vv(&mut peer_vv, &ops);
        on_progress(0.85);
        if ops.len() < page_size {
            break;
        }
    }
    on_progress(1.0);
    Ok(SyncRound {
        pulled,
        pushed,
        latency_ms,
    })
}

fn advance_vv(vv: &mut VersionVector, ops: &[Op]) {
    for op in ops {
        vv.entry(op.origin.clone())
            .and_modify(|seen| {
                if *seen < op.hlc {
                    *seen = op.hlc.clone();
                }
            })
            .or_insert_with(|| op.hlc.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        domain::Space,
        sync::{Hello, PullResponse, PushResponse},
    };
    use parking_lot::Mutex;

    struct FakeTransport {
        remote: Arc<Store>,
        page_size: usize,
        pushes: Mutex<usize>,
    }

    impl FakeTransport {
        fn new(remote: Arc<Store>, page_size: usize) -> Self {
            Self {
                remote,
                page_size,
                pushes: Mutex::new(0),
            }
        }
    }

    impl SyncTransport for FakeTransport {
        fn hello<'a>(&'a self, _peer: &'a Peer) -> crate::sync::BoxFuture<'a, SyncResult<Hello>> {
            Box::pin(async move {
                Ok(Hello {
                    computer_id: self.remote.computer_id().to_string(),
                    name: "remote".into(),
                    os: "test".into(),
                    version_vector: self.remote.version_vector()?,
                })
            })
        }

        fn pull<'a>(
            &'a self,
            _peer: &'a Peer,
            version_vector: VersionVector,
        ) -> crate::sync::BoxFuture<'a, SyncResult<PullResponse>> {
            Box::pin(async move {
                let ops = self.remote.ops_since(&version_vector, self.page_size)?;
                Ok(PullResponse {
                    more: ops.len() == self.page_size,
                    ops,
                    version_vector: self.remote.version_vector()?,
                })
            })
        }

        fn push<'a>(
            &'a self,
            _peer: &'a Peer,
            ops: Vec<Op>,
        ) -> crate::sync::BoxFuture<'a, SyncResult<PushResponse>> {
            Box::pin(async move {
                *self.pushes.lock() += 1;
                let applied = self.remote.apply_remote(&ops)?;
                Ok(PushResponse { applied })
            })
        }
    }

    fn space(id: &str, name: &str) -> Space {
        Space {
            id: id.into(),
            name: name.into(),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn fake_transport_converges_with_pagination() {
        let a = Arc::new(Store::open_in_memory().unwrap());
        let b = Arc::new(Store::open_in_memory().unwrap());
        for i in 0..2105 {
            a.put(&space(&format!("a{i}"), "from-a")).unwrap();
        }
        for i in 0..17 {
            b.put(&space(&format!("b{i}"), "from-b")).unwrap();
        }
        let peer = Peer {
            id: b.computer_id().into(),
            name: "b".into(),
            ..Default::default()
        };
        let transport = FakeTransport::new(b.clone(), 2000);
        let round = sync_peer_with_page_size(a.clone(), &transport, &peer, 2000, |_| {})
            .await
            .unwrap();
        assert!(round.pushed > 2000);
        assert!(round.pulled > 0);
        assert_eq!(a.version_vector().unwrap(), b.version_vector().unwrap());
        assert_eq!(
            a.list::<Space>().unwrap().len(),
            b.list::<Space>().unwrap().len()
        );
        assert!(*transport.pushes.lock() > 1);
    }

    #[tokio::test]
    async fn legacy_copy_policy_migrates_after_all_paginated_pull_ops() {
        let local = Arc::new(Store::open_in_memory().unwrap());
        let remote = Arc::new(Store::open_in_memory().unwrap());
        let clock = crate::store::HlcClock::new("legacy".into());
        let legacy_ops = [
            Op {
                hlc: clock.now().encode(),
                origin: "legacy".into(),
                kind: "space".into(),
                entity_id: "space".into(),
                field: "name".into(),
                value: serde_json::json!("Travel"),
            },
            Op {
                hlc: clock.now().encode(),
                origin: "legacy".into(),
                kind: "project".into(),
                entity_id: "project".into(),
                field: "space_id".into(),
                value: serde_json::json!("space"),
            },
            Op {
                hlc: clock.now().encode(),
                origin: "legacy".into(),
                kind: "project".into(),
                entity_id: "project".into(),
                field: "final_copies_required".into(),
                value: serde_json::json!(5),
            },
        ];
        remote.apply_remote(&legacy_ops).unwrap();

        let peer = Peer {
            id: remote.computer_id().into(),
            name: "legacy".into(),
            ..Default::default()
        };
        let transport = FakeTransport::new(remote.clone(), 1);
        sync_peer_with_page_size(local.clone(), &transport, &peer, 1, |_| {})
            .await
            .unwrap();

        assert_eq!(
            local
                .get::<Space>("space")
                .unwrap()
                .unwrap()
                .final_copies_required,
            5
        );
        assert_eq!(
            remote
                .get::<Space>("space")
                .unwrap()
                .unwrap()
                .final_copies_required,
            5
        );
    }
}
