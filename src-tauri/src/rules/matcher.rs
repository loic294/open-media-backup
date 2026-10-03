use super::RuleError;
use crate::domain::{PathRule, RuleSyntax};
use globset::{GlobBuilder, GlobMatcher};
use regex::{Regex, RegexBuilder};

enum Pattern {
    Glob(GlobMatcher),
    Regex(Regex),
}

impl Pattern {
    fn is_match(&self, text: &str) -> bool {
        match self {
            Pattern::Glob(g) => g.is_match(text),
            Pattern::Regex(r) => r.is_match(text),
        }
    }
}

#[derive(PartialEq)]
enum Scope {
    FoldersOnly,
    FullPath,
    AnyName,
}

pub(super) struct Matcher {
    pattern: Pattern,
    scope: Scope,
}

impl Matcher {
    pub fn new(rule: &PathRule) -> Result<Self, RuleError> {
        let raw = rule.pattern.trim();
        let err = |reason: String| RuleError {
            pattern: rule.pattern.clone(),
            reason,
        };
        let (text, scope) = match rule.syntax {
            RuleSyntax::Regex => (raw, Scope::AnyName),
            RuleSyntax::Glob if raw.ends_with('/') => {
                (raw.trim_end_matches('/'), Scope::FoldersOnly)
            }
            RuleSyntax::Glob if raw.contains('/') => (raw.trim_start_matches('/'), Scope::FullPath),
            RuleSyntax::Glob => (raw, Scope::AnyName),
        };
        let pattern = match rule.syntax {
            RuleSyntax::Glob => Pattern::Glob(
                GlobBuilder::new(text)
                    .case_insensitive(true)
                    .literal_separator(scope == Scope::FullPath)
                    .build()
                    .map_err(|e| err(e.to_string()))?
                    .compile_matcher(),
            ),
            RuleSyntax::Regex => Pattern::Regex(
                RegexBuilder::new(text)
                    .case_insensitive(true)
                    .build()
                    .map_err(|e| err(e.to_string()))?,
            ),
        };
        Ok(Self { pattern, scope })
    }

    pub fn matches(&self, rel_path: &str, folders: &[&str], file: &str) -> bool {
        match self.scope {
            Scope::FoldersOnly => folders.iter().any(|f| self.pattern.is_match(f)),
            Scope::FullPath => self.pattern.is_match(rel_path),
            Scope::AnyName => {
                self.pattern.is_match(file) || folders.iter().any(|f| self.pattern.is_match(f))
            }
        }
    }
}
