//! Include/exclude rules evaluated against folder names and file names.
//!
//! Semantics: rules are evaluated in order and the last matching rule wins.
//! Without any include rule everything is included by default.
//! - A pattern ending with `/` matches folder names only (e.g. `PRIVATE/`).
//! - A pattern containing `/` elsewhere matches the whole relative path.
//! - Otherwise it matches the file name or any folder name.
mod matcher;

use crate::domain::{FileRule, RuleAction};
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
    default_included: bool,
}

impl RuleSet {
    pub fn compile(rules: &[FileRule]) -> Result<Self, RuleError> {
        let compiled = rules
            .iter()
            .filter(|r| !r.pattern.trim().is_empty())
            .map(|r| Ok((r.action, Matcher::new(r)?)))
            .collect::<Result<Vec<_>, RuleError>>()?;
        let default_included = !compiled.iter().any(|(a, _)| *a == RuleAction::Include);
        Ok(Self {
            rules: compiled,
            default_included,
        })
    }

    /// `rel_path` uses `/` separators and is relative to the source folder.
    pub fn allows(&self, rel_path: &str) -> bool {
        let parts: Vec<&str> = rel_path.split('/').filter(|p| !p.is_empty()).collect();
        let Some((file, folders)) = parts.split_last() else {
            return false;
        };
        self.rules
            .iter()
            .fold(self.default_included, |included, (action, matcher)| {
                if matcher.matches(rel_path, folders, file) {
                    *action == RuleAction::Include
                } else {
                    included
                }
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::RuleSyntax;

    fn rule(action: RuleAction, syntax: RuleSyntax, pattern: &str) -> FileRule {
        FileRule {
            action,
            syntax,
            pattern: pattern.into(),
        }
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
        assert!(set.allows("DCIM/100MSDCF/DSC0001.ARW"));
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
    fn invalid_regex_is_reported() {
        let err = RuleSet::compile(&[rule(Include, Regex, "(")])
            .err()
            .unwrap();
        assert_eq!(err.pattern, "(");
    }
}
