use super::entity::impl_entity;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Project {
    pub id: String,
    pub space_id: String,
    pub name: String,
    pub values: BTreeMap<String, String>,
    /// Number of verified copies on "final" devices required before a card can be wiped.
    pub final_copies_required: u32,
    pub archived: bool,
}
impl_entity!(Project, Project);

impl Default for Project {
    fn default() -> Self {
        Self {
            id: String::new(),
            space_id: String::new(),
            name: String::new(),
            values: BTreeMap::new(),
            final_copies_required: 2,
            archived: false,
        }
    }
}
