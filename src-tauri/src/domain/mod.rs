//! Pure domain types shared by the store, engines and Tauri commands.
mod device;
mod entity;
mod file;
mod flow;
mod project;
mod rule;
mod source;
mod space;

pub use device::{Computer, Device, DeviceKind, DeviceMapping, DeviceRole};
pub use entity::{Entity, EntityKind};
pub use file::{FileCopy, FileRecord, SafeCopyOverride};
pub use flow::{Destination, DestinationKind, Flow, RemoteHash};
pub use project::{validate_project_ranges, Project, ProjectGranularity};
pub use rule::{ConditionRuleKind, FileRule, PathRule, RuleAction, RuleExpr, RuleSyntax};
pub use source::{ProjectScope, Source};
pub use space::{HashAlgo, Space, VariableDef, VerifyMode};

pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}
