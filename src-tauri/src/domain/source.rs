use super::entity::impl_entity;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum ProjectScope {
    #[default]
    All,
    Selected {
        project_ids: Vec<String>,
    },
    None,
}

impl ProjectScope {
    pub fn allows(&self, project_id: &str) -> bool {
        match self {
            Self::All => true,
            Self::Selected { project_ids } => project_ids.iter().any(|id| id == project_id),
            Self::None => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Source {
    pub id: String,
    pub space_id: String,
    pub device_id: String,
    /// Folder relative to the device root; may contain `{variables}`.
    pub path_template: String,
    pub offer_wipe: bool,
    pub position: i64,
    pub project_scope: ProjectScope,
}
impl_entity!(Source, Source);

impl Source {
    pub fn validate_projects(&self, projects: &[super::Project]) -> Result<(), String> {
        if let ProjectScope::Selected { project_ids } = &self.project_scope {
            if project_ids.iter().any(|id| {
                !projects
                    .iter()
                    .any(|project| &project.id == id && project.space_id == self.space_id)
            }) {
                return Err(
                    "Source references a project outside its space or a missing project".into(),
                );
            }
        }
        Ok(())
    }
}
