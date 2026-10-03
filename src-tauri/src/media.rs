use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    Image,
    Video,
    Raw,
    Other,
}

const IMAGES: &[&str] = &[
    "jpg", "jpeg", "png", "webp", "heic", "heif", "tif", "tiff", "gif", "bmp",
];
const RAWS: &[&str] = &[
    "arw", "cr2", "cr3", "nef", "nrw", "dng", "raf", "orf", "rw2", "pef", "srw", "x3f", "3fr",
    "iiq",
];
const VIDEOS: &[&str] = &[
    "mp4", "mov", "m4v", "mts", "m2ts", "avi", "mkv", "mxf", "insv", "lrv", "3gp", "braw", "r3d",
];

pub fn media_kind(path: &Path) -> MediaKind {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if RAWS.contains(&ext.as_str()) {
        MediaKind::Raw
    } else if IMAGES.contains(&ext.as_str()) {
        MediaKind::Image
    } else if VIDEOS.contains(&ext.as_str()) {
        MediaKind::Video
    } else {
        MediaKind::Other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_extensions_case_insensitively() {
        assert_eq!(media_kind(Path::new("a/IMG_001.ARW")), MediaKind::Raw);
        assert_eq!(media_kind(Path::new("x.jpg")), MediaKind::Image);
        assert_eq!(media_kind(Path::new("C0001.MP4")), MediaKind::Video);
        assert_eq!(media_kind(Path::new("notes.xml")), MediaKind::Other);
    }
}
