use std::fs;
use std::io::{Cursor, Read};
use std::path::Path;

use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::{DynamicImage, ImageReader};

use crate::media::{media_kind, MediaKind};
use crate::thumbnails::embedded_jpeg::extract_embedded_jpegs;
use crate::thumbnails::orientation::{jpeg_orientation, Orientation};
use crate::thumbnails::ThumbnailError;

const MAX_EDGE: u32 = 360;
const JPEG_QUALITY: u8 = 80;
const EMBEDDED_SCAN_LIMIT: u64 = 64 * 1024 * 1024;
const LARGE_JPEG_FALLBACK: u64 = 8 * 1024 * 1024;

pub fn create_image_thumbnail(path: &Path, out: &Path) -> Result<bool, ThumbnailError> {
    let kind = media_kind(path);
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let metadata = fs::metadata(path).map_err(|source| ThumbnailError::Io {
        path: path.to_path_buf(),
        source,
    })?;

    if matches!(kind, MediaKind::Raw) || ext == "heic" || ext == "heif" {
        return create_from_embedded_preview(path, out);
    }

    let directly_supported = matches!(ext.as_str(), "jpg" | "jpeg" | "png" | "webp");
    if !directly_supported {
        return Ok(false);
    }

    if (ext == "jpg" || ext == "jpeg")
        && metadata.len() > LARGE_JPEG_FALLBACK
        && create_from_embedded_preview(path, out)?
    {
        return Ok(true);
    }

    let bytes = fs::read(path).map_err(|source| ThumbnailError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    create_from_bytes(&bytes, out, ext == "jpg" || ext == "jpeg")?;
    Ok(true)
}

pub fn create_from_embedded_preview(path: &Path, out: &Path) -> Result<bool, ThumbnailError> {
    let file = fs::File::open(path).map_err(|source| ThumbnailError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut limit = file.take(EMBEDDED_SCAN_LIMIT);
    let mut bytes = Vec::new();
    limit
        .read_to_end(&mut bytes)
        .map_err(|source| ThumbnailError::Io {
            path: path.to_path_buf(),
            source,
        })?;

    let mut best: Option<(usize, RangeBytes)> = None;
    for range in extract_embedded_jpegs(&bytes) {
        if range.len() <= best.as_ref().map_or(0, |(len, _)| *len) {
            continue;
        }
        if image::load_from_memory_with_format(&bytes[range.clone()], image::ImageFormat::Jpeg)
            .is_ok()
        {
            best = Some((
                range.len(),
                RangeBytes {
                    start: range.start,
                    end: range.end,
                },
            ));
        }
    }

    if let Some((_, range)) = best {
        create_from_bytes(&bytes[range.start..range.end], out, true)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

struct RangeBytes {
    start: usize,
    end: usize,
}

fn create_from_bytes(bytes: &[u8], out: &Path, orient_jpeg: bool) -> Result<(), ThumbnailError> {
    let mut image = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(ThumbnailError::ImageReader)?
        .decode()
        .map_err(ThumbnailError::Image)?;
    if orient_jpeg {
        image = apply_orientation(image, jpeg_orientation(bytes));
    }
    write_resized_jpeg(image, out)
}

fn apply_orientation(image: DynamicImage, orientation: Orientation) -> DynamicImage {
    match orientation {
        Orientation::Normal => image,
        Orientation::Rotate90 => image.rotate90(),
        Orientation::Rotate180 => image.rotate180(),
        Orientation::Rotate270 => image.rotate270(),
    }
}

fn write_resized_jpeg(image: DynamicImage, out: &Path) -> Result<(), ThumbnailError> {
    let thumb = if image.width() > MAX_EDGE || image.height() > MAX_EDGE {
        image.resize(MAX_EDGE, MAX_EDGE, FilterType::Triangle)
    } else {
        image
    }
    .into_rgb8();
    let file = fs::File::create(out).map_err(|source| ThumbnailError::Io {
        path: out.to_path_buf(),
        source,
    })?;
    let mut encoder = JpegEncoder::new_with_quality(file, JPEG_QUALITY);
    encoder.encode_image(&thumb).map_err(ThumbnailError::Image)
}
