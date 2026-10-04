//! Embedded media metadata for project assignment and the inspector.
//!
//! Call [`extract_metadata`] off the UI thread: video probing is blocking and
//! bounded to ten seconds. Missing fields serialize as `null`. Capture dates
//! never use filesystem timestamps or EXIF's modification-only `DateTime`.
//! Dimensions describe the stored pixels; orientation/rotation is separate.
//! Unsupported containers and malformed metadata return errors, not empty data.

mod capture;
mod image;
mod video;
mod video_fallback;
pub use capture::extract_capture_time;

use std::path::{Path, PathBuf};

use chrono::{DateTime, NaiveDateTime};
use serde::Serialize;

use crate::media::{media_kind, MediaKind};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MediaMetadata {
    pub media_type: MediaKind,
    pub size_bytes: u64,
    pub capture_time: Option<CaptureTime>,
    pub dimensions: Option<Dimensions>,
    pub camera: CameraMetadata,
    pub exposure: ExposureMetadata,
    pub orientation: Option<u32>,
    pub video: Option<VideoMetadata>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CaptureTime {
    /// ISO local wall time, with fractional seconds when embedded.
    pub local_datetime: String,
    /// `None` means the file does not specify a timezone; do not assume UTC.
    pub utc_offset_seconds: Option<i32>,
    pub source: CaptureTimeSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureTimeSource {
    ExifOriginal,
    ExifDigitized,
    VideoCreationTime,
    VideoOriginalDate,
    /// Camera XML sidecar, e.g. Sony `<clip>M01.XML` `CreationDate`.
    SidecarXml,
    /// ISO-BMFF `mvhd` creation time (UTC), read without ffprobe.
    Mp4Header,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Dimensions {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct CameraMetadata {
    pub make: Option<String>,
    pub model: Option<String>,
    pub lens_make: Option<String>,
    pub lens_model: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ExposureMetadata {
    pub shutter_seconds: Option<f64>,
    pub aperture_f_number: Option<f64>,
    pub iso: Option<u32>,
    pub focal_length_mm: Option<f64>,
    pub focal_length_35mm: Option<u32>,
    pub compensation_ev: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VideoMetadata {
    pub container: Option<String>,
    pub codec: Option<String>,
    pub pixel_format: Option<String>,
    pub duration_seconds: Option<f64>,
    pub frame_rate: Option<f64>,
    pub bit_rate_bps: Option<u64>,
    pub rotation_degrees: Option<i32>,
    pub audio: Vec<AudioMetadata>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AudioMetadata {
    pub codec: Option<String>,
    pub channels: Option<u32>,
    pub sample_rate_hz: Option<u32>,
}

#[derive(Debug, thiserror::Error)]
pub enum MetadataError {
    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("unsupported media metadata format at {path}")]
    Unsupported { path: PathBuf },
    #[error("EXIF parse error: {0}")]
    Exif(#[from] exif::Error),
    #[error("image metadata error: {0}")]
    Image(#[from] ::image::ImageError),
    #[error("invalid embedded metadata: {0}")]
    Invalid(String),
    #[error("failed to run ffprobe: {0}")]
    ProbeIo(#[source] std::io::Error),
    #[error("ffprobe timed out after ten seconds")]
    ProbeTimeout,
    #[error("ffprobe failed: {0}")]
    ProbeFailed(String),
    #[error("invalid ffprobe JSON: {0}")]
    ProbeJson(#[from] serde_json::Error),
}

/// Extracts metadata from a local regular file. Video uses `ffprobe`
/// (`OMB_FFPROBE`, PATH, Homebrew, or next to the executable, in that order);
/// when it is missing, fails, or finds no capture date, a camera XML sidecar
/// and the MP4 `mvhd` header fill in whatever fields are still missing.
/// A successful result with `capture_time == None` must remain unassigned.
pub fn extract_metadata(path: &Path) -> Result<MediaMetadata, MetadataError> {
    let stat = std::fs::metadata(path).map_err(|source| MetadataError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    if !stat.is_file() {
        return Err(MetadataError::Unsupported {
            path: path.to_path_buf(),
        });
    }
    let mut metadata = MediaMetadata {
        media_type: media_kind(path),
        size_bytes: stat.len(),
        capture_time: None,
        dimensions: None,
        camera: CameraMetadata::default(),
        exposure: ExposureMetadata::default(),
        orientation: None,
        video: None,
    };
    match metadata.media_type {
        MediaKind::Image | MediaKind::Raw => image::extract(path, &mut metadata)?,
        MediaKind::Video => video::extract(path, &mut metadata)?,
        MediaKind::Other => {
            return Err(MetadataError::Unsupported {
                path: path.to_path_buf(),
            })
        }
    }
    Ok(metadata)
}

fn parse_capture_time(text: &str, source: CaptureTimeSource) -> Result<CaptureTime, MetadataError> {
    let zoned = DateTime::parse_from_rfc3339(text)
        .or_else(|_| DateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S%.f%z"));
    if let Ok(date) = zoned {
        return Ok(CaptureTime {
            local_datetime: date
                .naive_local()
                .format("%Y-%m-%dT%H:%M:%S%.f")
                .to_string(),
            utc_offset_seconds: Some(date.offset().local_minus_utc()),
            source,
        });
    }
    for format in [
        "%Y:%m:%d %H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S%.f",
    ] {
        if let Ok(date) = NaiveDateTime::parse_from_str(text, format) {
            return Ok(CaptureTime {
                local_datetime: date.format("%Y-%m-%dT%H:%M:%S%.f").to_string(),
                utc_offset_seconds: None,
                source,
            });
        }
    }
    Err(MetadataError::Invalid(format!(
        "capture timestamp {text:?}"
    )))
}

#[cfg(test)]
mod tests;
