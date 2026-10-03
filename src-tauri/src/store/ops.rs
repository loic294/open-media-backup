use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// One field-level change of one entity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Op {
    pub hlc: String,
    pub origin: String,
    pub kind: String,
    pub entity_id: String,
    pub field: String,
    pub value: serde_json::Value,
}

/// Highest op HLC seen per origin computer.
pub type VersionVector = BTreeMap<String, String>;

pub const DELETED_FIELD: &str = "_deleted";
