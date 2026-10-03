use crate::store::{Op, VersionVector};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hello {
    pub computer_id: String,
    pub name: String,
    pub os: String,
    pub version_vector: VersionVector,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullRequest {
    pub version_vector: VersionVector,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullResponse {
    pub ops: Vec<Op>,
    pub more: bool,
    pub version_vector: VersionVector,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushRequest {
    pub ops: Vec<Op>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushResponse {
    pub applied: usize,
}
