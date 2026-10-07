//! Thumbnails that cameras already store next to their clips (read-only lookup).

use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

pub fn default_paths() -> Vec<String> {
    [
        "../THMBNL/{stem}T01.JPG",
        "../../MISC/THM/{folder}/{stem}.SCR",
        "../../MISC/THM/{folder}/{stem}.THM",
        "{stem}.THM",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

/// The first existing camera thumbnail for `video`, following `rules` in order.
pub fn find(video: &Path, rules: &[String]) -> Option<PathBuf> {
    let dir = video.parent()?;
    let stem = video.file_stem()?.to_str()?;
    let folder = dir.file_name().and_then(|f| f.to_str()).unwrap_or("");
    rules
        .iter()
        .filter_map(|rule| expand(rule, stem, folder))
        .filter_map(|relative| resolve(dir, &relative))
        .find(|candidate| candidate != video && is_jpeg(candidate))
}

fn expand(rule: &str, stem: &str, folder: &str) -> Option<PathBuf> {
    let rule = rule.trim();
    if rule.is_empty() {
        return None;
    }
    let path = PathBuf::from(rule.replace("{stem}", stem).replace("{folder}", folder));
    path.components()
        .all(|c| {
            matches!(
                c,
                Component::Normal(_) | Component::ParentDir | Component::CurDir
            )
        })
        .then_some(path)
}

/// Joins `relative` onto `dir`, matching the file name case-insensitively when needed.
fn resolve(dir: &Path, relative: &Path) -> Option<PathBuf> {
    let exact = normalize(&dir.join(relative));
    if exact.is_file() {
        return Some(exact);
    }
    let parent = exact.parent()?;
    let wanted = exact.file_name()?.to_str()?;
    fs::read_dir(parent)
        .ok()?
        .filter_map(Result::ok)
        .find(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.eq_ignore_ascii_case(wanted))
        })
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
}

fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
}

fn is_jpeg(path: &Path) -> bool {
    let mut magic = [0u8; 3];
    fs::File::open(path)
        .and_then(|mut file| file.read_exact(&mut magic))
        .is_ok()
        && magic == [0xFF, 0xD8, 0xFF]
}

#[cfg(test)]
mod tests {
    use super::*;

    const JPEG: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0, 0, 0];

    fn put(root: &Path, rel: &str, bytes: &[u8]) -> PathBuf {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn finds_sony_dji_and_sidecar_thumbnails_with_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let rules = default_paths();

        let sony = put(root, "PRIVATE/M4ROOT/CLIP/C7000.MP4", b"v");
        let sony_thumb = put(root, "PRIVATE/M4ROOT/THMBNL/C7000T01.JPG", JPEG);
        assert_eq!(find(&sony, &rules), Some(sony_thumb));

        let dji = put(root, "DCIM/DJI_001/DJI_0006_D.MP4", b"v");
        put(root, "MISC/THM/DJI_001/DJI_0006_D.THM", JPEG);
        let scr = put(root, "MISC/THM/DJI_001/DJI_0006_D.SCR", JPEG);
        assert_eq!(find(&dji, &rules), Some(scr.clone()));
        fs::remove_file(scr).unwrap();
        assert_eq!(
            find(&dji, &rules),
            Some(root.join("MISC/THM/DJI_001/DJI_0006_D.THM"))
        );

        let gopro = put(root, "DCIM/100GOPRO/GX010001.MP4", b"v");
        let thm = put(root, "DCIM/100GOPRO/gx010001.thm", JPEG);
        let found = find(&gopro, &rules).unwrap();
        assert!(found
            .to_string_lossy()
            .eq_ignore_ascii_case(&thm.to_string_lossy()));
    }

    #[test]
    fn ignores_missing_non_jpeg_and_absolute_rules() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let video = put(root, "CLIP/A.MP4", b"v");
        put(root, "CLIP/A.THM", b"not a jpeg");
        assert_eq!(find(&video, &default_paths()), None);
        let absolute = put(root, "abs.jpg", JPEG);
        assert_eq!(
            find(
                &video,
                &[absolute.to_string_lossy().into_owned(), " ".into()]
            ),
            None
        );
        assert_eq!(find(&video, &["{stem}.MP4".into()]), None);
    }
}
