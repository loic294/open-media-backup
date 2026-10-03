use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleAction {
    #[default]
    Include,
    Exclude,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleSyntax {
    #[default]
    Glob,
    Regex,
}

/// Include/exclude rule matched against folder names and file names.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct FileRule {
    pub action: RuleAction,
    pub syntax: RuleSyntax,
    pub pattern: String,
}
