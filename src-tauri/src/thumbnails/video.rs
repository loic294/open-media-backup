use std::env;
use std::fs;
use std::io::{self, Read};
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
const MAX_DIAGNOSTIC_BYTES: usize = 16 * 1024;

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
    create_video_frame_with_ffmpeg(&ffmpeg, path, out, max_edge)
}

fn create_video_frame_with_ffmpeg(
    ffmpeg: &Path,
    path: &Path,
    out: &Path,
    max_edge: u32,
) -> Result<bool, ThumbnailError> {
    run_ffmpeg(ffmpeg, path, out, "1", max_edge, TIMEOUT)?;
    if usable_output(out) {
        return Ok(true);
    }
    let _ = fs::remove_file(out);
    run_ffmpeg(ffmpeg, path, out, "0", max_edge, TIMEOUT)?;
    if usable_output(out) {
        Ok(true)
    } else {
        Err(ThumbnailError::FfmpegNoFrame)
    }
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
    timeout: Duration,
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
        .args([
            "-pix_fmt", "yuvj420p", "-c:v", "mjpeg", "-f", "image2", "-update", "1",
        ])
        .arg("-y")
        .arg(out)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }

    let mut child = command
        .spawn()
        .map_err(|source| ThumbnailError::FfmpegSpawn { source })?;
    let stderr = child.stderr.take().expect("piped ffmpeg stderr");
    // Keep draining after the limit so verbose failures cannot fill the pipe.
    let diagnostics = thread::spawn(move || read_diagnostics(stderr));
    let started = Instant::now();
    let result = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => {}
            Err(source) => break Err(ThumbnailError::FfmpegWait { source }),
        }
        if started.elapsed() >= timeout {
            break Err(ThumbnailError::FfmpegTimeout);
        }
        thread::sleep(Duration::from_millis(50));
    };
    if result.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    let stderr = diagnostics
        .join()
        .unwrap_or_else(|_| Err(io::Error::other("ffmpeg diagnostic reader panicked")));
    let status = result?;
    let stderr = stderr.map_err(|source| ThumbnailError::FfmpegDiagnostics { source })?;
    if !status.success() {
        return Err(ThumbnailError::FfmpegFailed {
            status: status.to_string(),
            stderr,
        });
    }
    Ok(())
}

fn read_diagnostics(mut reader: impl Read) -> io::Result<String> {
    let mut saved = Vec::new();
    let mut buffer = [0; 4096];
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        let keep = count.min(MAX_DIAGNOSTIC_BYTES - saved.len());
        saved.extend_from_slice(&buffer[..keep]);
    }
    Ok(String::from_utf8_lossy(&saved).trim().to_owned())
}

fn usable_output(path: &Path) -> bool {
    fs::metadata(path).map(|m| m.len() > 0).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostics_are_bounded_and_fully_drained() {
        let input = vec![b'x'; MAX_DIAGNOSTIC_BYTES * 4];
        let mut reader = io::Cursor::new(input);
        let captured = read_diagnostics(&mut reader).unwrap();
        assert_eq!(captured.len(), MAX_DIAGNOSTIC_BYTES);
        assert_eq!(reader.position(), (MAX_DIAGNOSTIC_BYTES * 4) as u64);
    }

    #[cfg(unix)]
    fn fake_ffmpeg(script: &str) -> (tempfile::TempDir, PathBuf) {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir_in(env::current_dir().unwrap()).unwrap();
        let executable = dir.path().join("ffmpeg");
        fs::write(&executable, format!("#!/bin/sh\n{script}\n")).unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
        (dir, executable)
    }

    #[cfg(unix)]
    #[test]
    fn runner_reports_nonzero_exit_with_bounded_stderr() {
        let (dir, executable) =
            fake_ffmpeg("printf 'decoder failed\\n' >&2\nhead -c 65536 /dev/zero >&2\nexit 7");
        let error = run_ffmpeg(
            &executable,
            &dir.path().join("input.mp4"),
            &dir.path().join("output.tmp"),
            "1",
            360,
            TIMEOUT,
        )
        .unwrap_err();
        match error {
            ThumbnailError::FfmpegFailed { status, stderr } => {
                assert!(status.contains('7'));
                assert!(stderr.starts_with("decoder failed"));
                assert_eq!(stderr.len(), MAX_DIAGNOSTIC_BYTES);
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[cfg(unix)]
    #[test]
    fn runner_times_out_and_reaps_process() {
        let (dir, executable) = fake_ffmpeg("exec sleep 5");
        let started = Instant::now();
        let error = run_ffmpeg(
            &executable,
            &dir.path().join("input.mp4"),
            &dir.path().join("output.tmp"),
            "1",
            360,
            Duration::from_millis(100),
        )
        .unwrap_err();
        assert!(matches!(error, ThumbnailError::FfmpegTimeout));
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[cfg(unix)]
    #[test]
    fn extraction_uses_explicit_jpeg_and_retries_empty_first_seek() {
        let (dir, executable) = fake_ffmpeg(
            r#"case " $* " in *" -c:v mjpeg -f image2 -update 1 "*) ;; *) exit 8 ;; esac
for out do :; done
case " $* " in
  *" -ss 1 "*) : > "$out" ;;
  *" -ss 0 "*) printf 'frame' > "$out" ;;
  *) exit 9 ;;
esac"#,
        );
        let out = dir.path().join("output.tmp");
        assert!(create_video_frame_with_ffmpeg(
            &executable,
            &dir.path().join("input.mp4"),
            &out,
            360,
        )
        .unwrap());
        assert_eq!(fs::read(out).unwrap(), b"frame");
    }

    #[cfg(unix)]
    #[test]
    fn successful_process_without_frames_reports_error() {
        let (dir, executable) = fake_ffmpeg("exit 0");
        let error = create_video_frame_with_ffmpeg(
            &executable,
            &dir.path().join("input.mp4"),
            &dir.path().join("output.tmp"),
            360,
        )
        .unwrap_err();
        assert!(matches!(error, ThumbnailError::FfmpegNoFrame));
    }

    #[test]
    fn missing_executable_reports_spawn_error() {
        let dir = tempfile::tempdir_in(env::current_dir().unwrap()).unwrap();
        let error = run_ffmpeg(
            &dir.path().join("missing-ffmpeg"),
            &dir.path().join("input.mp4"),
            &dir.path().join("output.tmp"),
            "1",
            360,
            TIMEOUT,
        )
        .unwrap_err();
        assert!(matches!(error, ThumbnailError::FfmpegSpawn { .. }));
    }
}
