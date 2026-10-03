use std::fs;
use std::io::{Cursor, Read};
use std::path::Path;

use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::{DynamicImage, ImageReader};

use crate::media::{media_kind, MediaKind};
use crate::thumbnails::embedded_jpeg::extract_embedded_jpegs;
use crate::thumbnails::jpeg_scaled;
use crate::thumbnails::orientation::{jpeg_orientation, Orientation};
use crate::thumbnails::ThumbnailError;

const MAX_EDGE: u32 = 360;
const JPEG_QUALITY: u8 = 80;
const EMBEDDED_SCAN_LIMIT: u64 = 64 * 1024 * 1024;
const PREVIEW_EDGE: u32 = 2560;

pub fn create_image_thumbnail(path: &Path, out: &Path) -> Result<bool, ThumbnailError> {
    create_image_thumbnail_at_edge(path, out, MAX_EDGE)
}

pub fn create_image_preview(path: &Path, out: &Path) -> Result<bool, ThumbnailError> {
    create_image_thumbnail_at_edge(path, out, PREVIEW_EDGE)
}

fn create_image_thumbnail_at_edge(
    path: &Path,
    out: &Path,
    max_edge: u32,
) -> Result<bool, ThumbnailError> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    if matches!(media_kind(path), MediaKind::Raw) || ext == "heic" || ext == "heif" {
        return create_from_embedded_preview(path, out, max_edge);
    }
    if !matches!(ext.as_str(), "jpg" | "jpeg" | "png" | "webp") {
        return Ok(false);
    }

    let bytes = fs::read(path).map_err(|source| ThumbnailError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    create_from_bytes(&bytes, out, ext == "jpg" || ext == "jpeg", max_edge)?;
    Ok(true)
}

/// Uses the smallest embedded JPEG preview that is still at least `MAX_EDGE` on its long side
/// (or the largest one if all are smaller). Only headers are parsed to choose.
pub fn create_from_embedded_preview(
    path: &Path,
    out: &Path,
    max_edge: u32,
) -> Result<bool, ThumbnailError> {
    let file = fs::File::open(path).map_err(|source| ThumbnailError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut bytes = Vec::new();
    file.take(EMBEDDED_SCAN_LIMIT)
        .read_to_end(&mut bytes)
        .map_err(|source| ThumbnailError::Io {
            path: path.to_path_buf(),
            source,
        })?;

    let candidates = extract_embedded_jpegs(&bytes)
        .into_iter()
        .filter_map(|r| jpeg_scaled::dimensions(&bytes[r.clone()]).map(|(w, h)| (w.max(h), r)));
    let best = candidates.fold(
        None::<(u32, std::ops::Range<usize>)>,
        |best, (edge, r)| match &best {
            None => Some((edge, r)),
            Some((b, _)) if (*b < max_edge && edge > *b) || (edge >= max_edge && edge < *b) => {
                Some((edge, r))
            }
            _ => best,
        },
    );

    match best {
        Some((_, range)) => create_from_bytes(&bytes[range], out, true, max_edge).map(|()| true),
        None => Ok(false),
    }
}

fn create_from_bytes(
    bytes: &[u8],
    out: &Path,
    is_jpeg: bool,
    max_edge: u32,
) -> Result<(), ThumbnailError> {
    let mut image = match is_jpeg
        .then(|| jpeg_scaled::decode_scaled(bytes, max_edge))
        .flatten()
    {
        Some(image) => image,
        None => ImageReader::new(Cursor::new(bytes))
            .with_guessed_format()
            .map_err(ThumbnailError::ImageReader)?
            .decode()
            .map_err(ThumbnailError::Image)?,
    };
    if is_jpeg {
        image = apply_orientation(image, jpeg_orientation(bytes));
    }
    write_resized_jpeg(image, out, max_edge)
}

fn apply_orientation(image: DynamicImage, orientation: Orientation) -> DynamicImage {
    match orientation {
        Orientation::Normal => image,
        Orientation::Rotate90 => image.rotate90(),
        Orientation::Rotate180 => image.rotate180(),
        Orientation::Rotate270 => image.rotate270(),
    }
}

fn write_resized_jpeg(
    image: DynamicImage,
    out: &Path,
    max_edge: u32,
) -> Result<(), ThumbnailError> {
    let thumb = if image.width() > max_edge || image.height() > max_edge {
        image.resize(max_edge, max_edge, FilterType::Triangle)
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
