//! ffprobe-free fallbacks for video metadata: camera XML sidecars (Sony
//! `<clip>M01.XML`) and the ISO-BMFF `moov/mvhd` header. Only fields that are
//! still missing are filled; existing values are never overwritten.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use chrono::DateTime;

use super::{
    parse_capture_time, CaptureTime, CaptureTimeSource, Dimensions, MediaMetadata, VideoMetadata,
};

const SIDECAR_LIMIT: u64 = 1024 * 1024;
const MAX_BOXES: usize = 4096;
/// Seconds between 1904-01-01 (QuickTime epoch) and 1970-01-01.
const QUICKTIME_EPOCH_OFFSET: u64 = 2_082_844_800;

/// Returns true when any field was filled from a fallback source.
pub(super) fn fill(path: &Path, metadata: &mut MediaMetadata) -> bool {
    let mut filled = false;
    if let Some(sidecar) = read_sidecar(path) {
        filled |= apply_sidecar(&sidecar, metadata);
    }
    if let Some(header) = read_mvhd(path) {
        filled |= apply_mvhd(header, metadata);
    }
    filled
}

fn sidecar_candidates(path: &Path) -> Vec<PathBuf> {
    let (Some(dir), Some(stem)) = (path.parent(), path.file_stem().and_then(|s| s.to_str())) else {
        return Vec::new();
    };
    ["M01.XML", "M01.xml", "m01.xml"]
        .iter()
        .map(|suffix| dir.join(format!("{stem}{suffix}")))
        .collect()
}

fn read_sidecar(path: &Path) -> Option<String> {
    sidecar_candidates(path).into_iter().find_map(|candidate| {
        let file = File::open(&candidate).ok()?;
        if !file.metadata().ok()?.is_file() {
            return None;
        }
        let mut text = String::new();
        file.take(SIDECAR_LIMIT).read_to_string(&mut text).ok()?;
        text.contains("NonRealTimeMeta").then_some(text)
    })
}

/// Finds the first `<element ...>` start tag and returns its attribute `name`.
fn attribute(xml: &str, element: &str, name: &str) -> Option<String> {
    let open = format!("<{element}");
    let mut rest = xml;
    while let Some(index) = rest.find(&open) {
        let after = &rest[index + open.len()..];
        let boundary = after.chars().next()?;
        if boundary.is_whitespace() || boundary == '/' || boundary == '>' {
            let tag = &after[..after.find('>')?];
            let mut attrs = tag;
            while let Some(position) = attrs.find(name) {
                let preceded = attrs[..position]
                    .chars()
                    .last()
                    .is_some_and(char::is_whitespace);
                let tail = attrs[position + name.len()..].trim_start();
                if preceded {
                    if let Some(tail) = tail.strip_prefix('=') {
                        let tail = tail.trim_start();
                        let quote = tail.chars().next()?;
                        if quote == '"' || quote == '\'' {
                            let value = &tail[1..];
                            let end = value.find(quote)?;
                            return Some(unescape(&value[..end]));
                        }
                    }
                }
                attrs = &attrs[position + name.len()..];
            }
            return None;
        }
        rest = after;
    }
    None
}

fn unescape(value: &str) -> String {
    value
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .trim()
        .to_owned()
}

fn sidecar_frame_rate(value: &str) -> Option<f64> {
    let rate: f64 = value.trim_end_matches(['p', 'i', 'P', 'I']).parse().ok()?;
    let rate = match rate {
        r if (r - 23.98).abs() < 0.01 => 24000.0 / 1001.0,
        r if (r - 29.97).abs() < 0.01 => 30000.0 / 1001.0,
        r if (r - 59.94).abs() < 0.01 => 60000.0 / 1001.0,
        r if (r - 119.88).abs() < 0.01 => 120000.0 / 1001.0,
        r => r,
    };
    (rate.is_finite() && rate > 0.0).then_some(rate)
}

fn sidecar_codec(value: &str) -> Option<String> {
    let family = value.split(['_', '@']).next()?.to_ascii_lowercase();
    match family.as_str() {
        "" => None,
        "avc" => Some("h264".into()),
        _ => Some(family),
    }
}

fn video_mut(metadata: &mut MediaMetadata) -> &mut VideoMetadata {
    metadata.video.get_or_insert_with(|| VideoMetadata {
        container: None,
        codec: None,
        pixel_format: None,
        duration_seconds: None,
        frame_rate: None,
        bit_rate_bps: None,
        rotation_degrees: None,
        audio: Vec::new(),
    })
}

fn set<T>(slot: &mut Option<T>, value: Option<T>) -> bool {
    if slot.is_none() && value.is_some() {
        *slot = value;
        true
    } else {
        false
    }
}

pub(super) fn apply_sidecar(xml: &str, metadata: &mut MediaMetadata) -> bool {
    let mut filled = false;
    let capture = attribute(xml, "CreationDate", "value")
        .and_then(|date| parse_capture_time(&date, CaptureTimeSource::SidecarXml).ok());
    filled |= set(&mut metadata.capture_time, capture);
    filled |= set(
        &mut metadata.camera.make,
        attribute(xml, "Device", "manufacturer"),
    );
    filled |= set(
        &mut metadata.camera.model,
        attribute(xml, "Device", "modelName"),
    );
    filled |= set(
        &mut metadata.camera.lens_model,
        attribute(xml, "Lens", "modelName"),
    );
    let width = attribute(xml, "VideoLayout", "pixel").and_then(|v| v.parse::<u32>().ok());
    let height =
        attribute(xml, "VideoLayout", "numOfVerticalLine").and_then(|v| v.parse::<u32>().ok());
    if let (Some(width), Some(height)) = (width, height) {
        if width > 0 && height > 0 {
            filled |= set(&mut metadata.dimensions, Some(Dimensions { width, height }));
        }
    }
    let rate = attribute(xml, "VideoFrame", "formatFps")
        .or_else(|| attribute(xml, "VideoFrame", "captureFps"))
        .and_then(|v| sidecar_frame_rate(&v));
    let codec = attribute(xml, "VideoFrame", "videoCodec").and_then(|v| sidecar_codec(&v));
    let frames = attribute(xml, "Duration", "value").and_then(|v| v.parse::<u64>().ok());
    let duration = frames.zip(rate).map(|(frames, rate)| frames as f64 / rate);
    if rate.is_some() || codec.is_some() || duration.is_some() {
        let video = video_mut(metadata);
        filled |= set(&mut video.frame_rate, rate);
        filled |= set(&mut video.codec, codec);
        filled |= set(&mut video.duration_seconds, duration);
    }
    filled
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Mvhd {
    pub creation_seconds: u64,
    pub timescale: u32,
    pub duration: u64,
}

fn read_box_header(file: &mut File, end: u64) -> Option<(u64, [u8; 4], u64)> {
    let start = file.stream_position().ok()?;
    if start + 8 > end {
        return None;
    }
    let mut header = [0u8; 8];
    file.read_exact(&mut header).ok()?;
    let size = u32::from_be_bytes(header[..4].try_into().ok()?) as u64;
    let kind: [u8; 4] = header[4..8].try_into().ok()?;
    let (size, header_len) = match size {
        0 => (end - start, 8),
        1 => {
            let mut large = [0u8; 8];
            file.read_exact(&mut large).ok()?;
            (u64::from_be_bytes(large), 16)
        }
        size => (size, 8),
    };
    if size < header_len || start.checked_add(size)? > end {
        return None;
    }
    Some((start, kind, start + size))
}

fn find_box(file: &mut File, start: u64, end: u64, kind: &[u8; 4]) -> Option<(u64, u64)> {
    file.seek(SeekFrom::Start(start)).ok()?;
    for _ in 0..MAX_BOXES {
        let (_, found, box_end) = read_box_header(file, end)?;
        if &found == kind {
            return Some((file.stream_position().ok()?, box_end));
        }
        file.seek(SeekFrom::Start(box_end)).ok()?;
    }
    None
}

pub(super) fn read_mvhd(path: &Path) -> Option<Mvhd> {
    let mut file = File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let (moov_start, moov_end) = find_box(&mut file, 0, len, b"moov")?;
    let (mvhd_start, mvhd_end) = find_box(&mut file, moov_start, moov_end, b"mvhd")?;
    file.seek(SeekFrom::Start(mvhd_start)).ok()?;
    let mut body = vec![0u8; (mvhd_end - mvhd_start).min(32) as usize];
    file.read_exact(&mut body).ok()?;
    let u32_at = |at: usize| Some(u32::from_be_bytes(body.get(at..at + 4)?.try_into().ok()?));
    let u64_at = |at: usize| Some(u64::from_be_bytes(body.get(at..at + 8)?.try_into().ok()?));
    match body.first()? {
        0 => Some(Mvhd {
            creation_seconds: u32_at(4)? as u64,
            timescale: u32_at(12)?,
            duration: u32_at(16)? as u64,
        }),
        1 => Some(Mvhd {
            creation_seconds: u64_at(4)?,
            timescale: u32_at(20)?,
            duration: u64_at(24)?,
        }),
        _ => None,
    }
}

fn mvhd_capture(header: Mvhd) -> Option<CaptureTime> {
    // Zero means "unset"; anything before 1970 is a camera clock that was never set.
    let unix = header
        .creation_seconds
        .checked_sub(QUICKTIME_EPOCH_OFFSET)?;
    if unix == 0 {
        return None;
    }
    let date = DateTime::from_timestamp(i64::try_from(unix).ok()?, 0)?;
    Some(CaptureTime {
        local_datetime: date.naive_utc().format("%Y-%m-%dT%H:%M:%S%.f").to_string(),
        utc_offset_seconds: Some(0),
        source: CaptureTimeSource::Mp4Header,
    })
}

pub(super) fn apply_mvhd(header: Mvhd, metadata: &mut MediaMetadata) -> bool {
    let mut filled = set(&mut metadata.capture_time, mvhd_capture(header));
    let duration =
        (header.timescale > 0 && header.duration != u32::MAX as u64 && header.duration != u64::MAX)
            .then(|| header.duration as f64 / header.timescale as f64)
            .filter(|d| d.is_finite() && *d > 0.0);
    if duration.is_some() {
        filled |= set(&mut video_mut(metadata).duration_seconds, duration);
    }
    filled
}
