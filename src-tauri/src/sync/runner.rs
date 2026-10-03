use crate::{
    store::Peer,
    sync::{sync_peer_with_page_size, PeerState, SyncError, SyncResult, SyncService, PAGE_SIZE},
};
use std::{
    sync::atomic::Ordering,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::task::JoinSet;

impl SyncService {
    pub async fn sync_now(&self) -> SyncResult<()> {
        let peers = self.inner.store.peers()?;
        self.inner.syncing.store(true, Ordering::SeqCst);
        self.emit_status();
        let mut set = JoinSet::new();
        for peer in peers {
            let service = self.clone();
            set.spawn(async move { service.sync_one(peer).await });
        }
        let mut first_error = None;
        while let Some(result) = set.join_next().await {
            match result {
                Ok(Err(err @ SyncError::Offline(_))) => {
                    log::debug!("peer offline during sync: {err}")
                }
                Ok(Err(err)) => {
                    if first_error.is_none() {
                        first_error = Some(err);
                    }
                }
                Err(err) => {
                    if first_error.is_none() {
                        first_error = Some(SyncError::Other(err.to_string()));
                    }
                }
                Ok(Ok(())) => {}
            }
        }
        self.inner.syncing.store(false, Ordering::SeqCst);
        self.emit_status();
        first_error.map_or(Ok(()), Err)
    }

    pub(crate) async fn sync_one(&self, mut peer: Peer) -> SyncResult<()> {
        self.set_peer_status(&peer, PeerState::Syncing, 0.0, None, None);
        let store = self.inner.store.clone();
        let transport = self.inner.transport.clone();
        let result =
            sync_peer_with_page_size(store, transport.as_ref(), &peer, PAGE_SIZE, |progress| {
                self.set_peer_status(&peer, PeerState::Syncing, progress, None, None);
            })
            .await;
        match result {
            Ok(round) => {
                peer.last_seen = Some(now_ms());
                peer.last_error = None;
                self.inner.store.save_peer(&peer)?;
                self.set_peer_status(
                    &peer,
                    PeerState::UpToDate,
                    1.0,
                    Some(round.latency_ms),
                    None,
                );
                Ok(())
            }
            Err(SyncError::Offline(msg)) => {
                peer.last_error = Some(msg.clone());
                self.inner.store.save_peer(&peer)?;
                self.set_peer_status(&peer, PeerState::Offline, 0.0, None, Some(msg.clone()));
                Err(SyncError::Offline(msg))
            }
            Err(err) => {
                let msg = err.to_string();
                peer.last_error = Some(msg.clone());
                self.inner.store.save_peer(&peer)?;
                self.set_peer_status(&peer, PeerState::Error, 0.0, None, Some(msg));
                Err(err)
            }
        }
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or(0)
}
