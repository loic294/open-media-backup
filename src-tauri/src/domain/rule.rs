use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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
pub struct PathRule {
    pub action: RuleAction,
    pub syntax: RuleSyntax,
    pub pattern: String,
}

/// Destination file rule.
///
/// Existing include/exclude rules remain serialized as `{ action, syntax, pattern }`.
/// Condition rules are serialized as `{ "kind": "condition", "expr": ... }` and
/// act as additional filters against the matching project's template variables.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FileRule {
    Condition {
        kind: ConditionRuleKind,
        expr: RuleExpr,
    },
    Path(PathRule),
}

impl Default for FileRule {
    fn default() -> Self {
        Self::Path(PathRule::default())
    }
}

impl FileRule {
    pub fn path(action: RuleAction, syntax: RuleSyntax, pattern: impl Into<String>) -> Self {
        Self::Path(PathRule {
            action,
            syntax,
            pattern: pattern.into(),
        })
    }

    pub fn condition(expr: RuleExpr) -> Self {
        Self::Condition {
            kind: ConditionRuleKind::Condition,
            expr,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConditionRuleKind {
    Condition,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum RuleExpr {
    Eq { var: String, value: String },
    Ne { var: String, value: String },
    Not { item: Box<RuleExpr> },
    And { items: Vec<RuleExpr> },
    Or { items: Vec<RuleExpr> },
}

impl RuleExpr {
    pub fn eval(&self, vars: &BTreeMap<String, String>) -> bool {
        match self {
            Self::Eq { var, value } => {
                normalized(vars.get(var).map_or("", String::as_str)) == normalized(value)
            }
            Self::Ne { var, value } => {
                normalized(vars.get(var).map_or("", String::as_str)) != normalized(value)
            }
            Self::Not { item } => !item.eval(vars),
            Self::And { items } => items.iter().all(|item| item.eval(vars)),
            Self::Or { items } => items.iter().any(|item| item.eval(vars)),
        }
    }
}

// Condition comparisons intentionally trim whitespace and ignore ASCII/Unicode
// case so user-entered project variable values behave like path rules. Missing
// variables are read as the empty string by RuleExpr::eval.
fn normalized(value: &str) -> String {
    value.trim().to_lowercase()
}
