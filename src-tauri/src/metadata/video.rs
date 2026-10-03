use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde::Deserialize;

use super::{
    parse_capture_time, AudioMetadata, CaptureTimeSource, Dimensions, MediaMetadata, MetadataError,
    VideoMetadata,
};

const OUTPUT_LIMIT: u64 = 4 * 1024 * 1024;

#[derive(Debug, Default, Deserialize)]
pub(super) struct Probe {
    #[serde(default)]
    streams: Vec<Stream>,
    #[serde(default)]
    format: Format,
}

#[derive(Debug, Default, Deserialize)]
struct Format {
    format_name: Option<String>,
    duration: Option<String>,
    bit_rate: Option<String>,
    #[serde(default)]
    tags: BTreeMap<String, String>,
}

#[derive(Debug, Default, Deserialize)]
struct Stream {
    codec_type: Option<String>,
    codec_name: Option<String>,
    pix_fmt: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    duration: Option<String>,
    avg_frame_rate: Option<String>,
    r_frame_rate: Option<String>,
    bit_rate: Option<String>,
    channels: Option<u32>,
    sample_rate: Option<String>,
    #[serde(default)]
    tags: BTreeMap<String, String>,
    #[serde(default)]
    side_data_list: Vec<SideData>,
    #[serde(default)]
    disposition: Disposition,
}

#[derive(Debug, Default, Deserialize)]
struct Disposition {
    #[serde(default)]
    attached_pic: u32,
}

#[derive(Debug, Deserialize)]
struct SideData {
    rotation: Option<i32>,
}

pub(super) fn extract(path: &Path, metadata: &mut MediaMetadata) -> Result<(), MetadataError> {
    let executable = find_ffprobe().ok_or_else(|| {
        MetadataError::ProbeIo(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "ffprobe not found; install it or set OMB_FFPROBE",
        ))
    })?;
    populate(
        run_probe(path, &executable, Duration::from_secs(10))?,
        metadata,
    )
}

fn run_probe(path: &Path, executable: &Path, timeout: Duration) -> Result<Probe, MetadataError> {
    let path = path.canonicalize().map_err(|source| MetadataError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut command = Command::new(executable);
    command
        .args([
            "-v",
            "error",
            "-protocol_whitelist",
            "file",
            "-show_format",
            "-show_streams",
            "-of",
            "json",
        ])
        .arg(&path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command.spawn().map_err(MetadataError::ProbeIo)?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| MetadataError::ProbeFailed("missing stdout".into()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| MetadataError::ProbeFailed("missing stderr".into()))?;
    // Drain both pipes concurrently so a verbose probe cannot deadlock.
    let out = thread::spawn(move || read_output(stdout));
    let err = thread::spawn(move || read_output(stderr));
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if started.elapsed() < timeout => {
                thread::sleep(Duration::from_millis(25));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(MetadataError::ProbeTimeout);
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(MetadataError::ProbeIo(error));
            }
        }
    };
    let stdout = out
        .join()
        .map_err(|_| MetadataError::ProbeFailed("stdout reader panicked".into()))?;
    let stderr = err
        .join()
        .map_err(|_| MetadataError::ProbeFailed("stderr reader panicked".into()))?;
    let status = status?;
    let stdout = stdout?;
    let stderr = stderr?;
    if !status.success() {
        return Err(MetadataError::ProbeFailed(format!(
            "{status}: {}",
            String::from_utf8_lossy(&stderr)
        )));
    }
    Ok(serde_json::from_slice(&stdout)?)
}

fn read_output(reader: impl Read) -> Result<Vec<u8>, MetadataError> {
    let mut bytes = Vec::new();
    reader
        .take(OUTPUT_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(MetadataError::ProbeIo)?;
    if bytes.len() as u64 > OUTPUT_LIMIT {
        return Err(MetadataError::ProbeFailed("output exceeds 4 MiB".into()));
    }
    Ok(bytes)
}

fn find_ffprobe() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("OMB_FFPROBE") {
        // An explicitly configured executable must not silently fall back.
        return Some(PathBuf::from(path));
    }
    let name = if cfg!(windows) {
        "ffprobe.exe"
    } else {
        "ffprobe"
    };
    let mut candidates = Vec::new();
    if let Some(paths) = std::env::var_os("PATH") {
        candidates.extend(std::env::split_paths(&paths).map(|dir| dir.join(name)));
    }
    candidates.extend([
        PathBuf::from("/opt/homebrew/bin/ffprobe"),
        PathBuf::from("/usr/local/bin/ffprobe"),
    ]);
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join(name));
        }
    }
    candidates.into_iter().find(|path| path.is_file())
}

fn number<T: std::str::FromStr>(
    value: Option<&str>,
    name: &str,
) -> Result<Option<T>, MetadataError> {
    match value {
        None | Some("N/A") => Ok(None),
        Some(value) => value
            .parse()
            .map(Some)
            .map_err(|_| MetadataError::Invalid(format!("invalid {name}: {value:?}"))),
    }
}

fn nonnegative(value: Option<&str>, name: &str) -> Result<Option<f64>, MetadataError> {
    let value = number::<f64>(value, name)?;
    if value.is_some_and(|v| !v.is_finite() || v < 0.0) {
        return Err(MetadataError::Invalid(format!("invalid {name}")));
    }
    Ok(value)
}

fn frame_rate(value: Option<&str>) -> Result<Option<f64>, MetadataError> {
    let Some(value) = value.filter(|v| *v != "N/A" && *v != "0/0") else {
        return Ok(None);
    };
    let Some((num, den)) = value.split_once('/') else {
        return Err(MetadataError::Invalid(format!(
            "invalid frame rate {value:?}"
        )));
    };
    let num = nonnegative(Some(num), "frame rate numerator")?
        .ok_or_else(|| MetadataError::Invalid("missing frame rate numerator".into()))?;
    let den = nonnegative(Some(den), "frame rate denominator")?
        .ok_or_else(|| MetadataError::Invalid("missing frame rate denominator".into()))?;
    if den == 0.0 || !(num / den).is_finite() {
        return Err(MetadataError::Invalid(format!(
            "invalid frame rate {value:?}"
        )));
    }
    Ok((num > 0.0).then_some(num / den))
}

pub(super) fn populate(probe: Probe, metadata: &mut MediaMetadata) -> Result<(), MetadataError> {
    let stream = probe
        .streams
        .iter()
        .find(|s| s.codec_type.as_deref() == Some("video") && s.disposition.attached_pic == 0)
        .ok_or_else(|| MetadataError::Invalid("container has no video stream".into()))?;
    if let (Some(width), Some(height)) = (stream.width, stream.height) {
        if width == 0 || height == 0 {
            return Err(MetadataError::Invalid("zero video dimensions".into()));
        }
        metadata.dimensions = Some(Dimensions { width, height });
    }
    let mut tags: BTreeMap<String, String> = probe
        .format
        .tags
        .iter()
        .map(|(key, value)| (key.to_ascii_lowercase(), value.clone()))
        .collect();
    tags.extend(
        stream
            .tags
            .iter()
            .map(|(key, value)| (key.to_ascii_lowercase(), value.clone())),
    );
    let tag = |names: &[&str]| {
        names
            .iter()
            .find_map(|name| tags.get(*name).map(|v| v.trim()).filter(|v| !v.is_empty()))
    };
    if let Some(date) = tag(&[
        "com.apple.quicktime.creationdate",
        "date_time_original",
        "datetimeoriginal",
    ]) {
        metadata.capture_time = Some(parse_capture_time(
            date,
            CaptureTimeSource::VideoOriginalDate,
        )?);
    } else if let Some(date) = tag(&["creation_time"]) {
        metadata.capture_time = Some(parse_capture_time(
            date,
            CaptureTimeSource::VideoCreationTime,
        )?);
    }
    metadata.camera.make = tag(&["com.apple.quicktime.make", "make"]).map(str::to_owned);
    metadata.camera.model = tag(&["com.apple.quicktime.model", "model"]).map(str::to_owned);
    metadata.camera.lens_make = tag(&["lens_make"]).map(str::to_owned);
    metadata.camera.lens_model =
        tag(&["com.apple.quicktime.lensmodel", "lens_model", "lens"]).map(str::to_owned);
    metadata.exposure.shutter_seconds = nonnegative(tag(&["exposure_time"]), "exposure time")?;
    metadata.exposure.aperture_f_number = nonnegative(tag(&["f_number"]), "f number")?;
    metadata.exposure.iso = number(tag(&["iso"]), "ISO")?;
    metadata.exposure.focal_length_mm = nonnegative(tag(&["focal_length"]), "focal length")?;
    metadata.exposure.focal_length_35mm = number(tag(&["focal_length_35mm"]), "35mm focal length")?;
    metadata.exposure.compensation_ev =
        number(tag(&["exposure_compensation"]), "exposure compensation")?;
    if metadata
        .exposure
        .compensation_ev
        .is_some_and(|v| !v.is_finite())
    {
        return Err(MetadataError::Invalid(
            "invalid exposure compensation".into(),
        ));
    }
    let avg_rate = frame_rate(stream.avg_frame_rate.as_deref())?;
    let rate = match avg_rate {
        Some(rate) => Some(rate),
        None => frame_rate(stream.r_frame_rate.as_deref())?,
    };
    let rotation = stream
        .side_data_list
        .iter()
        .find_map(|s| s.rotation)
        .map(|value| Ok(Some(value)))
        .unwrap_or_else(|| number(tag(&["rotate"]), "rotation"))?;
    let audio = probe
        .streams
        .iter()
        .filter(|s| s.codec_type.as_deref() == Some("audio"))
        .map(|s| {
            Ok(AudioMetadata {
                codec: s.codec_name.clone(),
                channels: s.channels,
                sample_rate_hz: number(s.sample_rate.as_deref(), "sample rate")?,
            })
        })
        .collect::<Result<Vec<_>, MetadataError>>()?;
    let duration = match nonnegative(stream.duration.as_deref(), "duration")? {
        Some(value) => Some(value),
        None => nonnegative(probe.format.duration.as_deref(), "duration")?,
    };
    let bit_rate = match number(stream.bit_rate.as_deref(), "bit rate")? {
        Some(value) => Some(value),
        None => number(probe.format.bit_rate.as_deref(), "bit rate")?,
    };
    metadata.video = Some(VideoMetadata {
        container: probe.format.format_name,
        codec: stream.codec_name.clone(),
        pixel_format: stream.pix_fmt.clone(),
        duration_seconds: duration,
        frame_rate: rate,
        bit_rate_bps: bit_rate,
        rotation_degrees: rotation,
        audio,
    });
    Ok(())
}

#[cfg(all(test, unix))]
mod process_tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    fn probe_script(script: &str) -> Result<Probe, MetadataError> {
        probe_script_with_timeout(script, Duration::from_secs(5))
    }

    fn probe_script_with_timeout(script: &str, timeout: Duration) -> Result<Probe, MetadataError> {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("synthetic.mp4");
        std::fs::write(&input, []).unwrap();
        let executable = dir.path().join("ffprobe-stub");
        std::fs::write(&executable, format!("#!/bin/sh\n{script}\n")).unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
        run_probe(&input, &executable, timeout)
    }

    #[test]
    fn parses_subprocess_output() {
        let probe = probe_script(
            r#"printf '%s' '{"streams":[{"codec_type":"video","width":16,"height":16}]}'"#,
        )
        .unwrap();
        assert_eq!(probe.streams[0].width, Some(16));
    }

    #[test]
    fn subprocess_failures_include_stderr() {
        let error = probe_script("printf 'invalid container' >&2\nexit 1").unwrap_err();
        assert!(
            matches!(error, MetadataError::ProbeFailed(ref message) if message.contains("invalid container"))
        );
        assert!(matches!(
            probe_script("printf 'not json'"),
            Err(MetadataError::ProbeJson(_))
        ));
    }

    #[test]
    fn missing_executable_is_an_explicit_error() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("synthetic.mp4");
        std::fs::write(&input, []).unwrap();
        assert!(matches!(
            run_probe(
                &input,
                &dir.path().join("missing-ffprobe"),
                Duration::from_secs(1)
            ),
            Err(MetadataError::ProbeIo(_))
        ));
    }

    #[test]
    fn hung_probe_is_terminated_and_reaped() {
        let started = Instant::now();
        assert!(matches!(
            probe_script_with_timeout("exec sleep 30", Duration::from_secs(1)),
            Err(MetadataError::ProbeTimeout)
        ));
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn probe_output_is_bounded() {
        assert!(matches!(
            read_output(std::io::Cursor::new(vec![b' '; OUTPUT_LIMIT as usize + 1])),
            Err(MetadataError::ProbeFailed(_))
        ));
    }
}
