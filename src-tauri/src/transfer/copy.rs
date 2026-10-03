use super::handle::{Cancelled, JobHandle};
use crate::domain::{HashAlgo, VerifyMode};
use crate::hashing::{hash_file, hasher, BUFFER_SIZE};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CopyError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("hash mismatch after copy ({expected} ≠ {actual})")]
    HashMismatch { expected: String, actual: String },
    #[error("source changed since it was catalogued")]
    SourceChanged,
    #[error("cancelled")]
    Cancelled,
}

impl From<Cancelled> for CopyError {
    fn from(_: Cancelled) -> Self {
        CopyError::Cancelled
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CopyOutcome {
    pub hash: String,
    pub final_path: PathBuf,
    /// The destination already held an identical file; nothing was copied.
    pub adopted: bool,
}

/// Copies `src` to `dst` with hashing. Identical existing files are adopted;
/// different ones are kept and the copy gets a ` (n)` suffix.
pub fn copy_verified(
    src: &Path,
    dst: &Path,
    algo: HashAlgo,
    verify: VerifyMode,
    known_hash: Option<&str>,
    handle: &JobHandle,
) -> Result<CopyOutcome, CopyError> {
    let mut target = dst.to_path_buf();
    if target.exists() {
        let src_hash = match known_hash {
            Some(h) => h.to_string(),
            None => hash_checked(src, algo, handle)?,
        };
        if hash_checked(&target, algo, handle)? == src_hash {
            return Ok(CopyOutcome { hash: src_hash, final_path: target, adopted: true });
        }
        target = free_name(dst);
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    let partial = partial_path(&target);
    let result = stream_copy(src, &partial, algo, handle).and_then(|hash| {
        if known_hash.is_some_and(|k| k != hash) {
            return Err(CopyError::SourceChanged);
        }
        if let Ok(modified) = fs::metadata(src).and_then(|m| m.modified()) {
            let _ = File::options().write(true).open(&partial).and_then(|f| f.set_modified(modified));
        }
        fs::rename(&partial, &target)?;
        if verify == VerifyMode::Reread {
            let actual = hash_checked(&target, algo, handle)?;
            if actual != hash {
                let _ = fs::remove_file(&target);
                return Err(CopyError::HashMismatch { expected: hash, actual });
            }
        }
        Ok(hash)
    });
    match result {
        Ok(hash) => Ok(CopyOutcome { hash, final_path: target, adopted: false }),
        Err(e) => {
            let _ = fs::remove_file(&partial);
            Err(e)
        }
    }
}

fn stream_copy(src: &Path, partial: &Path, algo: HashAlgo, handle: &JobHandle) -> Result<String, CopyError> {
    let mut input = File::open(src)?;
    let mut output = OpenOptions::new().write(true).create(true).truncate(true).open(partial)?;
    let mut state = hasher(algo);
    let mut buf = vec![0u8; BUFFER_SIZE];
    loop {
        handle.checkpoint()?;
        let n = input.read(&mut buf)?;
        if n == 0 {
            break;
        }
        state.update(&buf[..n]);
        output.write_all(&buf[..n])?;
        handle.add_bytes(n as u64);
    }
    output.sync_all()?;
    Ok(state.finish_hex())
}

fn hash_checked(path: &Path, algo: HashAlgo, handle: &JobHandle) -> Result<String, CopyError> {
    hash_file(path, algo, |_| handle.checkpoint().is_ok()).map_err(|e| {
        if e.kind() == std::io::ErrorKind::Interrupted {
            CopyError::Cancelled
        } else {
            e.into()
        }
    })
}

fn partial_path(target: &Path) -> PathBuf {
    let mut name = target.file_name().unwrap_or_default().to_os_string();
    name.push(".omb-partial");
    target.with_file_name(name)
}

fn free_name(path: &Path) -> PathBuf {
    let stem = path.file_stem().unwrap_or_default().to_string_lossy().into_owned();
    let ext = path.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    (1..)
        .map(|n| path.with_file_name(format!("{stem} ({n}){ext}")))
        .find(|p| !p.exists())
        .expect("unbounded range")
}
