use std::path::{Component, Path, PathBuf};

/// Joins a `/`-separated relative path onto `root`, dropping `..`, `.` and absolute prefixes.
pub fn join_relative(root: &Path, relative: &str) -> PathBuf {
    let mut out = root.to_path_buf();
    for part in relative.split(['/', '\\']) {
        match part {
            "" | "." | ".." => {}
            p if p.contains(':') && cfg!(windows) => {}
            p => out.push(p),
        }
    }
    out
}

/// Path of `path` relative to `root`, using `/` separators. None if outside root.
pub fn to_relative(root: &Path, path: &Path) -> Option<String> {
    let rel = path.strip_prefix(root).ok()?;
    let parts: Vec<String> = rel
        .components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();
    Some(parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_is_confined_to_root() {
        let root = Path::new("/vol");
        assert_eq!(join_relative(root, "a/../b//c"), Path::new("/vol/a/b/c"));
        assert_eq!(
            join_relative(root, "/etc/passwd"),
            Path::new("/vol/etc/passwd")
        );
        assert_eq!(join_relative(root, "a\\b"), Path::new("/vol/a/b"));
    }

    #[test]
    fn relative_uses_forward_slashes() {
        let root = Path::new("/vol");
        assert_eq!(
            to_relative(root, &root.join("DCIM").join("A.ARW")).as_deref(),
            Some("DCIM/A.ARW")
        );
        assert_eq!(to_relative(root, Path::new("/other/x")), None);
    }
}
