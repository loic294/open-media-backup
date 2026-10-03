use crate::{
    store::{Peer, Store},
    sync::{
        address, server, HttpTransport, PeerState, PeerStatus, SyncError, SyncResult, SyncStatus,
        SyncTransport,
    },
};
use parking_lot::Mutex;
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};

#[derive(Clone)]
pub struct SyncService {
    pub(crate) inner: Arc<Inner>,
}

pub(crate) struct Inner {
    pub(crate) store: Arc<Store>,
    pub(crate) transport: Arc<dyn SyncTransport>,
    pub(crate) on_status: Arc<dyn Fn(SyncStatus) + Send + Sync>,
    pub(crate) statuses: Mutex<HashMap<String, PeerStatus>>,
    pub(crate) bound_addr: Mutex<Option<SocketAddr>>,
    pub(crate) syncing: AtomicBool,
}

impl SyncService {
    pub fn new(store: Arc<Store>, on_status: impl Fn(SyncStatus) + Send + Sync + 'static) -> Self {
        Self::with_transport(store, Arc::new(HttpTransport::default()), on_status)
    }

    pub(crate) fn with_transport(
        store: Arc<Store>,
        transport: Arc<dyn SyncTransport>,
        on_status: impl Fn(SyncStatus) + Send + Sync + 'static,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                store,
                transport,
                on_status: Arc::new(on_status),
                statuses: Mutex::new(HashMap::new()),
                bound_addr: Mutex::new(None),
                syncing: AtomicBool::new(false),
            }),
        }
    }

    pub async fn start_server(&self, port: u16) -> SyncResult<SocketAddr> {
        let addr = server::start_server(self.inner.store.clone(), self.token(), port).await?;
        *self.inner.bound_addr.lock() = Some(addr);
        self.emit_status();
        Ok(addr)
    }

    pub fn token(&self) -> String {
        if let Ok(Some(token)) = self.inner.store.setting("sync_token") {
            return token;
        }
        let token = uuid::Uuid::new_v4().simple().to_string();
        if let Err(err) = self.inner.store.set_setting("sync_token", &token) {
            log::error!("failed to save sync token: {err}");
        }
        token
    }

    pub async fn add_peer(&self, address: &str, token: &str) -> SyncResult<Peer> {
        let address = address::normalize_address(address)?;
        let probe = Peer {
            address: address.clone(),
            token: token.to_string(),
            ..Default::default()
        };
        let hello = self.inner.transport.hello(&probe).await?;
        if hello.computer_id == self.inner.store.computer_id() {
            return Err(SyncError::InvalidPeer(
                "cannot add this computer as a peer".into(),
            ));
        }
        let peer = Peer {
            id: hello.computer_id,
            name: hello.name,
            address,
            token: token.to_string(),
            last_seen: None,
            last_error: None,
        };
        self.inner.store.save_peer(&peer)?;
        self.set_peer_status(&peer, PeerState::Idle, 0.0, None, None);
        let _ = self.sync_one(peer.clone()).await;
        Ok(peer)
    }

    pub fn remove_peer(&self, id: &str) -> SyncResult<()> {
        self.inner.store.remove_peer(id)?;
        self.inner.statuses.lock().remove(id);
        self.emit_status();
        Ok(())
    }

    pub fn spawn_auto_sync(&self, interval: Arc<dyn Fn() -> Option<Duration> + Send + Sync>) {
        crate::sync::auto::spawn(self.clone(), interval);
    }

    pub(crate) fn subscribe(&self) -> tokio::sync::broadcast::Receiver<crate::store::ChangeEvent> {
        self.inner.store.subscribe()
    }
}
