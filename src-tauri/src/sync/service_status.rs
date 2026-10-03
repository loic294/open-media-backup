use crate::{
    domain::Computer,
    store::Peer,
    sync::{address, PeerState, PeerStatus, SyncService, SyncStatus},
};
use std::sync::atomic::Ordering;

impl SyncService {
    pub fn status(&self) -> SyncStatus {
        let peers = self.status_peers();
        let syncing = self.inner.syncing.load(Ordering::SeqCst);
        let progress = if peers.is_empty() {
            if syncing {
                0.0
            } else {
                1.0
            }
        } else {
            peers.iter().map(|p| p.progress).sum::<f32>() / peers.len() as f32
        };
        SyncStatus {
            listen_address: address::best_effort_listen_address(*self.inner.bound_addr.lock()),
            token: self.token(),
            syncing,
            progress,
            peers,
        }
    }

    pub(crate) fn emit_status(&self) {
        (self.inner.on_status)(self.status());
    }

    pub(crate) fn set_peer_status(
        &self,
        peer: &Peer,
        state: PeerState,
        progress: f32,
        latency_ms: Option<u64>,
        message: Option<String>,
    ) {
        let mut status = self.peer_status(peer);
        status.state = state;
        status.progress = progress;
        if latency_ms.is_some() {
            status.latency_ms = latency_ms;
        }
        if message.is_some() {
            status.message = message;
        } else if matches!(
            state,
            PeerState::Syncing | PeerState::UpToDate | PeerState::Idle
        ) {
            status.message = None;
        }
        self.inner.statuses.lock().insert(peer.id.clone(), status);
        self.emit_status();
    }

    fn status_peers(&self) -> Vec<PeerStatus> {
        let peers = self.inner.store.peers().unwrap_or_default();
        let existing = self.inner.statuses.lock();
        peers
            .iter()
            .map(|peer| {
                existing
                    .get(&peer.id)
                    .cloned()
                    .unwrap_or_else(|| self.peer_status(peer))
            })
            .collect()
    }

    fn peer_status(&self, peer: &Peer) -> PeerStatus {
        let computer = self.inner.store.get::<Computer>(&peer.id).ok().flatten();
        PeerStatus {
            id: peer.id.clone(),
            name: computer
                .as_ref()
                .map(|c| c.name.clone())
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| peer.name.clone()),
            address: peer.address.clone(),
            os: computer
                .as_ref()
                .map(|c| c.os.clone())
                .filter(|os| !os.is_empty())
                .unwrap_or_default(),
            state: if peer.last_error.is_some() {
                PeerState::Error
            } else {
                PeerState::Idle
            },
            progress: 0.0,
            last_synced: peer.last_seen,
            latency_ms: None,
            message: peer.last_error.clone(),
        }
    }
}
