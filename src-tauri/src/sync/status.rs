use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PeerState {
    Idle,
    Syncing,
    UpToDate,
    Offline,
    Error,
}

#[derive(Debug, Clone, Serialize)]
pub struct PeerStatus {
    pub id: String,
    pub name: String,
    pub address: String,
    pub os: String,
    pub state: PeerState,
    pub progress: f32,
    pub last_synced: Option<i64>,
    pub latency_ms: Option<u64>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncStatus {
    pub listen_address: String,
    pub token: String,
    pub syncing: bool,
    pub progress: f32,
    pub peers: Vec<PeerStatus>,
}
