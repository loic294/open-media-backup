//! Include/exclude and condition rules evaluated for destination files.
//!
//! Semantics: rules are evaluated in order and the last matching rule wins.
//! Without any include rule everything is included by default.
//! Condition rules are additional filters: every condition expression must be
//! true for the matching project's variables or the file is ignored. This mirrors
//! include/exclude rules by only narrowing the destination's eligible files.
//! - A pattern ending with `/` matches folder names only (e.g. `PRIVATE/`).
//! - A pattern containing `/` elsewhere matches the whole relative path.
//! - Otherwise it matches the file name or any folder name.
mod matcher;

use crate::domain::{FileRule, RuleAction, RuleExpr};
use crate::paths::TemplateVars;
use matcher::Matcher;
use thiserror::Error;

#[derive(Debug, Error)]
#[error("invalid rule \"{pattern}\": {reason}")]
pub struct RuleError {
    pub pattern: String,
    pub reason: String,
}

pub struct RuleSet {
    rules: Vec<(RuleAction, Matcher)>,
    conditions: Vec<RuleExpr>,
    default_included: bool,
}

impl RuleSet {
    pub fn compile(rules: &[FileRule]) -> Result<Self, RuleError> {
        let conditions = rules
            .iter()
            .filter_map(|rule| match rule {
                FileRule::Condition { expr, .. } => Some(expr.clone()),
                FileRule::Path(_) => None,
            })
            .collect();
        let compiled = rules
            .iter()
            .filter_map(|rule| match rule {
                FileRule::Path(path) if !path.pattern.trim().is_empty() => {
                    Some(Matcher::new(path).map(|matcher| (path.action, matcher)))
                }
                FileRule::Path(_) | FileRule::Condition { .. } => None,
            })
            .collect::<Result<Vec<_>, RuleError>>()?;
        let default_included = !compiled.iter().any(|(a, _)| *a == RuleAction::Include);
        Ok(Self {
            rules: compiled,
            conditions,
            default_included,
        })
    }

    /// `rel_path` uses `/` separators and is relative to the source folder.
    pub fn allows(&self, rel_path: &str) -> bool {
        self.allows_with_vars(rel_path, &TemplateVars::new())
    }

    /// `vars` are the template/project variables for this file's matching project.
    /// Missing projects should pass an empty map; missing variables compare as "".
    pub fn allows_with_vars(&self, rel_path: &str, vars: &TemplateVars) -> bool {
        let parts: Vec<&str> = rel_path.split('/').filter(|p| !p.is_empty()).collect();
        let Some((file, folders)) = parts.split_last() else {
            return false;
        };
        let path_included =
            self.rules
                .iter()
                .fold(self.default_included, |included, (action, matcher)| {
                    if matcher.matches(rel_path, folders, file) {
                        *action == RuleAction::Include
                    } else {
                        included
                    }
                });
        path_included && self.conditions.iter().all(|expr| expr.eval(vars))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::RuleSyntax;

    fn rule(action: RuleAction, syntax: RuleSyntax, pattern: &str) -> FileRule {
        FileRule::path(action, syntax, pattern)
    }
    use RuleAction::{Exclude, Include};
    use RuleSyntax::{Glob, Regex};

    #[test]
    fn no_rules_includes_everything() {
        assert!(RuleSet::compile(&[]).unwrap().allows("DCIM/A.ARW"));
    }

    #[test]
    fn design_example() {
        let set = RuleSet::compile(&[
            rule(Include, Glob, "*"),
            rule(Exclude, Glob, "PRIVATE/"),
            rule(Exclude, Glob, "*.THM"),
        ])
        .unwrap();
        assert!(set.allows("DCIM/100MSDCF/IMG_001.ARW"));
        assert!(!set.allows("PRIVATE/M4ROOT/CLIP/C0001.MP4"));
        assert!(!set.allows("DCIM/100MSDCF/c0001.thm"));
    }

    #[test]
    fn include_only_extensions() {
        let set = RuleSet::compile(&[rule(Include, Glob, "*.{arw,mp4}")]).unwrap();
        assert!(set.allows("x/a.ARW"));
        assert!(!set.allows("x/a.xml"));
    }

    #[test]
    fn folder_only_pattern_does_not_match_files() {
        let set = RuleSet::compile(&[rule(Exclude, Glob, "PRIVATE/")]).unwrap();
        assert!(set.allows("DCIM/PRIVATE"));
        assert!(!set.allows("PRIVATE/a.jpg"));
    }

    #[test]
    fn path_patterns_and_regex() {
        let set = RuleSet::compile(&[
            rule(Exclude, Glob, "DCIM/**/thumbs/*"),
            rule(Exclude, Regex, r"^\._"),
        ])
        .unwrap();
        assert!(!set.allows("DCIM/100/thumbs/a.jpg"));
        assert!(!set.allows("DCIM/._DSC1.ARW"));
        assert!(set.allows("DCIM/100/DSC1.ARW"));
    }

    #[test]
    fn escaped_glob_metacharacters_match_literal_file_names() {
        let set = RuleSet::compile(&[rule(Exclude, Glob, r"IMG_\[01\]\{raw\}\?.JPG")]).unwrap();
        assert!(!set.allows("DCIM/IMG_[01]{raw}?.JPG"));
        assert!(set.allows("DCIM/IMG_Araw1.JPG"));
    }

    #[test]
    fn invalid_regex_is_reported() {
        let err = RuleSet::compile(&[rule(Include, Regex, "(")])
            .err()
            .unwrap();
        assert_eq!(err.pattern, "(");
    }

    #[test]
    fn condition_filters_after_path_rules() {
        let set = RuleSet::compile(&[
            rule(Include, Glob, "*.jpg"),
            FileRule::condition(RuleExpr::Eq {
                var: "client".into(),
                value: "Acme".into(),
            }),
        ])
        .unwrap();
        let vars = TemplateVars::from([("client".into(), " acme ".into())]);
        assert!(set.allows_with_vars("DCIM/A.JPG", &vars));
        assert!(!set.allows_with_vars("DCIM/A.ARW", &vars));
        assert!(!set.allows_with_vars("DCIM/A.JPG", &TemplateVars::new()));
    }

    #[test]
    fn condition_expr_supports_eq_ne_not_and_or_missing_and_normalized_values() {
        let vars = TemplateVars::from([
            ("client".into(), " Acme ".into()),
            ("status".into(), "Ready".into()),
        ]);
        let expr = RuleExpr::And {
            items: vec![
                RuleExpr::Eq {
                    var: "client".into(),
                    value: "acme".into(),
                },
                RuleExpr::Not {
                    item: Box::new(RuleExpr::Eq {
                        var: "status".into(),
                        value: "draft".into(),
                    }),
                },
                RuleExpr::Or {
                    items: vec![
                        RuleExpr::Ne {
                            var: "missing".into(),
                            value: "present".into(),
                        },
                        RuleExpr::Eq {
                            var: "status".into(),
                            value: "blocked".into(),
                        },
                    ],
                },
            ],
        };
        assert!(expr.eval(&vars));
        assert!(RuleExpr::Eq {
            var: "missing".into(),
            value: "".into(),
        }
        .eval(&vars));
    }

    #[test]
    fn condition_rule_json_round_trips_and_legacy_rule_deserializes() {
        let legacy: FileRule =
            serde_json::from_str(r#"{"action":"exclude","syntax":"glob","pattern":"*.THM"}"#)
                .unwrap();
        assert_eq!(legacy, rule(Exclude, Glob, "*.THM"));
        let condition = FileRule::condition(RuleExpr::Not {
            item: Box::new(RuleExpr::Eq {
                var: "client".into(),
                value: "Acme".into(),
            }),
        });
        let json = serde_json::to_value(&condition).unwrap();
        assert_eq!(json["kind"], "condition");
        assert_eq!(json["expr"]["op"], "not");
        assert_eq!(serde_json::from_value::<FileRule>(json).unwrap(), condition);
    }
}
