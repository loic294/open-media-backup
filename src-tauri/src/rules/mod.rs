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
    rules: Vec<(RuleAction, String, Matcher)>,
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
                FileRule::Path(path) if !path.pattern.trim().is_empty() => Some(
                    Matcher::new(path).map(|matcher| (path.action, path.pattern.clone(), matcher)),
                ),
                FileRule::Path(_) | FileRule::Condition { .. } => None,
            })
            .collect::<Result<Vec<_>, RuleError>>()?;
        let default_included = !compiled
            .iter()
            .any(|(action, _, _)| *action == RuleAction::Include);
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
        self.ignore_reason(rel_path, vars).is_none()
    }

    pub fn ignore_reason(&self, rel_path: &str, vars: &TemplateVars) -> Option<String> {
        let parts: Vec<&str> = rel_path.split('/').filter(|p| !p.is_empty()).collect();
        let Some((file, folders)) = parts.split_last() else {
            return Some("The file has no relative path".into());
        };
        let mut path_included = self.default_included;
        let mut matching_rule = None;
        for (action, pattern, matcher) in &self.rules {
            if matcher.matches(rel_path, folders, file) {
                path_included = *action == RuleAction::Include;
                matching_rule = Some((*action, pattern));
            }
        }
        if !path_included {
            return Some(match matching_rule {
                Some((RuleAction::Exclude, pattern)) => {
                    format!("Excluded by destination rule \"{pattern}\"")
                }
                _ => "No include rule matched".into(),
            });
        }
        self.conditions
            .iter()
            .find_map(|expr| describe_failure(expr, vars))
            .map(|reason| format!("Condition not met: {reason}"))
    }
}

fn describe_failure(expr: &RuleExpr, vars: &TemplateVars) -> Option<String> {
    match expr {
        RuleExpr::Eq { .. } | RuleExpr::Ne { .. } if !expr.eval(vars) => Some(describe_expr(expr)),
        RuleExpr::Not { item } if item.eval(vars) => Some(describe_expr(expr)),
        RuleExpr::And { items } => {
            let failures: Vec<_> = items
                .iter()
                .filter_map(|item| describe_failure(item, vars))
                .collect();
            (!failures.is_empty()).then(|| failures.join(" AND "))
        }
        RuleExpr::Or { items } if !expr.eval(vars) => Some(format!(
            "No alternative matched: {}",
            items
                .iter()
                .map(describe_expr)
                .collect::<Vec<_>>()
                .join(" OR ")
        )),
        _ => None,
    }
}

fn describe_expr(expr: &RuleExpr) -> String {
    match expr {
        RuleExpr::Eq { var, value } => format!("{var} = {}", quote_value(value)),
        RuleExpr::Ne { var, value } => format!("{var} != {}", quote_value(value)),
        RuleExpr::Not { item } => format!("NOT ({})", describe_expr(item)),
        RuleExpr::And { items } => describe_expr_group(items, " AND "),
        RuleExpr::Or { items } => describe_expr_group(items, " OR "),
    }
}

fn describe_expr_group(items: &[RuleExpr], joiner: &str) -> String {
    format!(
        "({})",
        items
            .iter()
            .map(describe_expr)
            .collect::<Vec<_>>()
            .join(joiner)
    )
}

fn quote_value(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
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
        assert!(!set.allows("private/a.jpg"));
    }

    #[test]
    fn glob_rules_match_without_regard_to_case() {
        let set = RuleSet::compile(&[
            rule(Exclude, Glob, "*.ARW"),
            rule(Exclude, Glob, "PRIVATE/"),
        ])
        .unwrap();
        assert!(!set.allows("DCIM/img_0001.arw"));
        assert!(!set.allows("private/MIXED.JPG"));
        assert!(set.allows("DCIM/img_0001.jpg"));
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
    fn ignore_reason_identifies_the_effective_path_rule_and_condition() {
        let path_rule = RuleSet::compile(&[
            rule(Include, Glob, "DCIM/*.jpg"),
            rule(Exclude, Glob, "DCIM/"),
            rule(Include, Glob, "DCIM/KEEP.JPG"),
        ])
        .unwrap();
        assert_eq!(
            path_rule.ignore_reason("DCIM/SKIP.JPG", &TemplateVars::new()),
            Some("Excluded by destination rule \"DCIM/\"".into())
        );
        assert_eq!(
            path_rule.ignore_reason("OTHER/KEEP.JPG", &TemplateVars::new()),
            Some("No include rule matched".into())
        );

        let condition = RuleSet::compile(&[FileRule::condition(RuleExpr::Eq {
            var: "client".into(),
            value: "Acme".into(),
        })])
        .unwrap();
        assert_eq!(
            condition.ignore_reason("DCIM/A.JPG", &TemplateVars::new()),
            Some("Condition not met: client = \"Acme\"".into())
        );

        let compound = RuleSet::compile(&[FileRule::condition(RuleExpr::And {
            items: vec![
                RuleExpr::Eq {
                    var: "client".into(),
                    value: "Acme".into(),
                },
                RuleExpr::Eq {
                    var: "status".into(),
                    value: "Ready".into(),
                },
            ],
        })])
        .unwrap();
        assert_eq!(
            compound.ignore_reason(
                "DCIM/A.JPG",
                &TemplateVars::from([
                    ("client".into(), "Other".into()),
                    ("status".into(), "Ready".into())
                ])
            ),
            Some("Condition not met: client = \"Acme\"".into())
        );
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
