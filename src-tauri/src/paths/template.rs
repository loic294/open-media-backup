use chrono::{Datelike, Local};
use std::collections::BTreeMap;
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum TemplateError {
    #[error("missing value for {}", .0.iter().map(|v| format!("{{{v}}}")).collect::<Vec<_>>().join(", "))]
    Missing(Vec<String>),
    #[error("unclosed '{{' in \"{0}\"")]
    Unclosed(String),
}

pub type TemplateVars = BTreeMap<String, String>;

/// Expands `{name}` placeholders. Built-ins: `{date}` (YYYY-MM-DD), `{year}`, `{month}`, `{day}`.
pub fn expand(template: &str, vars: &TemplateVars) -> Result<String, TemplateError> {
    let mut out = String::with_capacity(template.len());
    let mut missing = Vec::new();
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let end = after
            .find('}')
            .ok_or_else(|| TemplateError::Unclosed(template.to_string()))?;
        let name = after[..end].trim();
        match lookup(name, vars) {
            Some(value) => out.push_str(&value),
            None => missing.push(name.to_string()),
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    if missing.is_empty() {
        Ok(out)
    } else {
        Err(TemplateError::Missing(missing))
    }
}

fn lookup(name: &str, vars: &TemplateVars) -> Option<String> {
    if let Some(value) = vars.get(name).filter(|v| !v.is_empty()) {
        return Some(value.clone());
    }
    let today = Local::now();
    match name {
        "date" => Some(today.format("%Y-%m-%d").to_string()),
        "year" => Some(today.year().to_string()),
        "month" => Some(format!("{:02}", today.month())),
        "day" => Some(format!("{:02}", today.day())),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(pairs: &[(&str, &str)]) -> TemplateVars {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn expands_variables() {
        let v = vars(&[
            ("project_name", "Trip_2026"),
            ("backup_folder", "2026/Travel"),
        ]);
        assert_eq!(
            expand("photo/{backup_folder}/{project_name}", &v).unwrap(),
            "photo/2026/Travel/Trip_2026"
        );
    }

    #[test]
    fn reports_all_missing_variables() {
        let err = expand("{a}/{b}/{ c }", &vars(&[("b", "x")])).unwrap_err();
        assert_eq!(err, TemplateError::Missing(vec!["a".into(), "c".into()]));
        assert_eq!(err.to_string(), "missing value for {a}, {c}");
    }

    #[test]
    fn builtin_dates_and_unclosed() {
        assert_eq!(expand("{year}", &vars(&[])).unwrap().len(), 4);
        assert_eq!(expand("{date}", &vars(&[])).unwrap().len(), 10);
        assert!(matches!(
            expand("a/{b", &vars(&[])),
            Err(TemplateError::Unclosed(_))
        ));
    }

    #[test]
    fn empty_value_counts_as_missing() {
        assert!(expand("{a}", &vars(&[("a", "")])).is_err());
    }
}
