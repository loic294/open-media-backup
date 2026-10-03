use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::thread;
use std::time::{Duration, Instant};

use crate::thumbnails::ThumbnailError;

static FFMPEG: OnceLock<Option<PathBuf>> = OnceLock::new();
const TIMEOUT: Duration = Duration::from_secs(10);
const THUMBNAIL_EDGE: u32 = 360;
const PREVIEW_EDGE: u32 = 2560;

pub fn create_video_thumbnail(path: &Path, out: &Path) -> Result<bool, ThumbnailError> {
    create_video_frame(path, out, THUMBNAIL_EDGE)
}

pub fn create_video_preview(path: &Path, out: &Path) -> Result<bool, ThumbnailError> {
    create_video_frame(path, out, PREVIEW_EDGE)
}

fn create_video_frame(path: &Path, out: &Path, max_edge: u32) -> Result<bool, ThumbnailError> {
    let Some(ffmpeg) = ffmpeg_path() else {
        return Ok(false);
    };
    run_ffmpeg(&ffmpeg, path, out, "1", max_edge)?;
    if usable_output(out) {
        return Ok(true);
    }
    let _ = fs::remove_file(out);
    run_ffmpeg(&ffmpeg, path, out, "0", max_edge)?;
    Ok(usable_output(out))
}

pub(super) fn ffmpeg_path() -> Option<PathBuf> {
    FFMPEG.get_or_init(find_ffmpeg).clone()
}

fn find_ffmpeg() -> Option<PathBuf> {
    if let Ok(path) = env::var("OMB_FFMPEG") {
        let path = PathBuf::from(path);
        if is_executable_file(&path) {
            return Some(path);
        }
    }

    let exe_name = if cfg!(windows) {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    };
    if let Some(paths) = env::var_os("PATH") {
        for dir in env::split_paths(&paths) {
            let candidate = dir.join(exe_name);
            if is_executable_file(&candidate) {
                return Some(candidate);
            }
        }
    }

    for candidate in [
        PathBuf::from("/opt/homebrew/bin/ffmpeg"),
        PathBuf::from("/usr/local/bin/ffmpeg"),
    ] {
        if is_executable_file(&candidate) {
            return Some(candidate);
        }
    }

    if let Ok(exe) = env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join(exe_name);
            if is_executable_file(&candidate) {
                return Some(candidate);
            }
        }
    }

    None
}

fn is_executable_file(path: &Path) -> bool {
    path.is_file()
}

fn run_ffmpeg(
    ffmpeg: &Path,
    input: &Path,
    out: &Path,
    seek: &str,
    max_edge: u32,
) -> Result<(), ThumbnailError> {
    let scale = format!("scale={max_edge}:-2");
    let mut command = Command::new(ffmpeg);
    command
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-ss")
        .arg(seek)
        .arg("-i")
        .arg(input)
        .arg("-frames:v")
        .arg("1")
        .arg("-vf")
        .arg(scale)
        .arg("-y")
        .arg(out)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }

    let mut child = command
        .spawn()
        .map_err(|source| ThumbnailError::FfmpegSpawn { source })?;
    let started = Instant::now();
    loop {
        if child
            .try_wait()
            .map_err(|source| ThumbnailError::FfmpegSpawn { source })?
            .is_some()
        {
            return Ok(());
        }
        if started.elapsed() >= TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            return Err(ThumbnailError::FfmpegTimeout);
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn usable_output(path: &Path) -> bool {
    fs::metadata(path).map(|m| m.len() > 0).unwrap_or(false)
}
