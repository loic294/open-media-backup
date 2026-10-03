mod address;
mod auto;
mod protocol;
mod runner;
mod server;
mod service;
mod service_status;
mod session;
mod status;
#[cfg(test)]
mod tests;
mod transport;

pub use address::{best_effort_listen_address, normalize_address};
pub use protocol::{Hello, PullRequest, PullResponse, PushRequest, PushResponse};
pub use service::SyncService;
pub(crate) use session::sync_peer_with_page_size;
pub use session::{sync_peer, SyncRound};
pub use status::{PeerState, PeerStatus, SyncStatus};
pub use transport::{BoxFuture, HttpTransport, SyncTransport};

pub const DEFAULT_PORT: u16 = 47821;
pub const PAGE_SIZE: usize = 2000;
pub const MAX_BODY_BYTES: usize = 32 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error("store error: {0}")]
    Store(#[from] crate::store::StoreError),
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("server error: {0}")]
    Io(#[from] std::io::Error),
    #[error("unauthorized")]
    Unauthorized,
    #[error("peer is offline: {0}")]
    Offline(String),
    #[error("invalid peer: {0}")]
    InvalidPeer(String),
    #[error("address error: {0}")]
    Address(String),
    #[error("sync error: {0}")]
    Other(String),
}

pub type SyncResult<T> = Result<T, SyncError>;

pub(crate) fn constant_time_eq(a: &str, b: &str) -> bool {
    let a = a.as_bytes();
    let b = b.as_bytes();
    let mut diff = a.len() ^ b.len();
    for i in 0..a.len().max(b.len()) {
        let av = a.get(i).copied().unwrap_or(0);
        let bv = b.get(i).copied().unwrap_or(0);
        diff |= usize::from(av ^ bv);
    }
    diff == 0
}
