use super::*;
use image::{GenericImageView, ImageBuffer, Rgb};
use std::io::Write;

fn tempdir() -> tempfile::TempDir {
    tempfile::tempdir_in(std::env::current_dir().expect("cwd")).expect("tempdir")
}

fn write_image(path: &Path, width: u32, height: u32) {
    let image = ImageBuffer::from_fn(width, height, |x, y| {
        Rgb([(x % 255) as u8, (y % 255) as u8, 128])
    });
    image.save(path).expect("save image");
}

#[test]
fn creates_and_reuses_cached_image_thumbnail() {
    let dir = tempdir();
    let src = dir.path().join("photo.jpg");
    let cache_dir = dir.path().join("cache");
    write_image(&src, 640, 480);

    let cache = ThumbnailCache::new(cache_dir);
    let first = cache
        .get_or_create(&src)
        .expect("thumbnail")
        .expect("supported");
    let first_meta = fs::metadata(&first).expect("metadata");
    let second = cache
        .get_or_create(&src)
        .expect("thumbnail")
        .expect("cached");

    assert_eq!(first, second);
    assert_eq!(
        first_meta.len(),
        fs::metadata(second).expect("metadata").len()
    );
    let thumb = image::open(first).expect("open thumb");
    assert!(thumb.width() <= 360 && thumb.height() <= 360);
}

#[test]
fn media_preview_keeps_large_image_resolution_and_uses_a_distinct_cache() {
    let dir = tempdir();
    let src = dir.path().join("photo.jpg");
    let cache = ThumbnailCache::new(dir.path().join("cache"));
    write_image(&src, 1200, 800);

    let thumbnail = cache.get_or_create(&src).unwrap().unwrap();
    let preview = cache.get_or_create_preview(&src).unwrap().unwrap();

    assert_ne!(thumbnail, preview);
    assert_eq!(image::open(thumbnail).unwrap().dimensions(), (360, 240));
    assert_eq!(image::open(preview).unwrap().dimensions(), (1200, 800));
}

#[test]
fn unsupported_media_returns_none() {
    let dir = tempdir();
    let src = dir.path().join("notes.txt");
    fs::write(&src, b"hello").expect("write");
    let cache = ThumbnailCache::new(dir.path().join("cache"));
    assert!(cache.get_or_create(&src).expect("thumbnail").is_none());
}

#[test]
fn raw_file_uses_largest_embedded_jpeg() {
    let dir = tempdir();
    let small = dir.path().join("small.jpg");
    let large = dir.path().join("large.jpg");
    let raw = dir.path().join("image.arw");
    write_image(&small, 16, 12);
    write_image(&large, 96, 80);

    let mut file = fs::File::create(&raw).expect("raw");
    file.write_all(b"noise").expect("noise");
    file.write_all(&fs::read(&small).expect("small"))
        .expect("small");
    file.write_all(b"middle").expect("middle");
    file.write_all(&fs::read(&large).expect("large"))
        .expect("large");
    file.write_all(b"tail").expect("tail");

    let cache = ThumbnailCache::new(dir.path().join("cache"));
    let thumb = cache
        .get_or_create(&raw)
        .expect("thumbnail")
        .expect("supported");
    let decoded = image::open(thumb).expect("open thumb");
    assert_eq!(decoded.dimensions(), (96, 80));
}

#[test]
fn invalid_video_reports_ffmpeg_failure_and_leaves_no_cache_files() {
    if super::video::ffmpeg_path().is_none() {
        return;
    }
    let dir = tempdir();
    let src = dir.path().join("clip.mp4");
    fs::write(&src, b"not a video").expect("write");
    let cache_dir = dir.path().join("cache");
    let cache = ThumbnailCache::new(cache_dir.clone());
    let error = cache.get_or_create(&src).expect_err("invalid video");
    assert!(matches!(error, ThumbnailError::FfmpegFailed { .. }));
    assert!(fs::read_dir(cache_dir).unwrap().next().is_none());
}

#[test]
fn video_media_preview_contains_a_large_frame_when_ffmpeg_is_available() {
    let Some(ffmpeg) = super::video::ffmpeg_path() else {
        return;
    };
    let dir = tempdir();
    let src = dir.path().join("clip.mp4");
    let generated = std::process::Command::new(ffmpeg)
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=size=640x480:rate=10:duration=0.2",
            "-c:v",
            "mpeg4",
            "-y",
        ])
        .arg(&src)
        .output()
        .expect("start ffmpeg");
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );

    let cache = ThumbnailCache::new(dir.path().join("cache"));
    let thumbnail = cache
        .get_or_create(&src)
        .expect("thumbnail")
        .expect("video frame");
    assert_eq!(image::open(&thumbnail).unwrap().dimensions(), (360, 270));
    assert_eq!(cache.get_or_create(&src).unwrap().unwrap(), thumbnail);
    let preview = cache
        .get_or_create_preview(&src)
        .expect("preview")
        .expect("video frame");
    let image = image::open(preview).expect("JPEG preview");
    assert_eq!(image.dimensions(), (2560, 1920));
}

#[test]
fn video_thumbnail_can_extract_a_frame_from_reported_clip() {
    let Some(path) = std::env::var_os("OMB_TEST_VIDEO") else {
        return;
    };
    assert!(super::video::ffmpeg_path().is_some(), "FFmpeg required");
    let dir = tempdir();
    let cache = ThumbnailCache::new(dir.path().join("cache"));
    let src = Path::new(&path);
    let thumbnail = cache.get_or_create(src).unwrap().expect("thumbnail");
    let decoded = image::open(&thumbnail).expect("JPEG thumbnail");
    assert_eq!(decoded.width(), 360);
    assert!(decoded.height() > 0);
    assert_eq!(cache.get_or_create(src).unwrap().unwrap(), thumbnail);
    let preview = cache.get_or_create_preview(src).unwrap().expect("preview");
    assert_ne!(thumbnail, preview);
    assert_eq!(image::open(preview).unwrap().width(), 2560);
}
