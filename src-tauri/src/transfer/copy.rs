use super::handle::{Cancelled, ConflictDecision, ConflictInfo, JobHandle};
use super::AnalysisPhase;
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
    /// The user chose to leave the differing destination file untouched; nothing was copied.
    pub skipped: bool,
    /// A differing destination file was replaced by a verified staged copy.
    pub replaced: bool,
    pub skip_evidence: Option<SkipEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkipEvidence {
    pub source_hash: String,
    pub source_size: u64,
    pub destination_hash: String,
}

/// Result of comparing a source file with the content at its expected destination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Comparison {
    Missing,
    Match(String),
    Different {
        source_hash: String,
        destination_hash: String,
    },
}

/// Hashes the actual bytes of both files. `source_hash` caches the source digest within one
/// operation; it is never taken from the catalog, which may predate a same-size edit.
pub fn compare_files(
    src: &Path,
    dst: &Path,
    algo: HashAlgo,
    handle: &JobHandle,
    source_hash: &mut Option<String>,
) -> Result<Comparison, CopyError> {
    compare_files_using(src, dst, handle, source_hash, &mut |path, source| {
        hash_checked_side(path, algo, handle, source)
    })
}

pub(super) fn compare_files_with_progress(
    src: &Path,
    dst: &Path,
    algo: HashAlgo,
    handle: &JobHandle,
    source_hash: &mut Option<String>,
    mut on_bytes: impl FnMut(bool, u64, u64),
) -> Result<Comparison, CopyError> {
    compare_files_using(src, dst, handle, source_hash, &mut |path, source| {
        hash_checked_with_progress(path, algo, handle, source, |bytes, total| {
            on_bytes(source, bytes, total);
        })
    })
}

fn compare_files_using(
    src: &Path,
    dst: &Path,
    handle: &JobHandle,
    source_hash: &mut Option<String>,
    hash: &mut impl FnMut(&Path, bool) -> Result<String, CopyError>,
) -> Result<Comparison, CopyError> {
    handle.checkpoint()?;
    match fs::metadata(dst) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Comparison::Missing),
        Err(e) => return Err(e.into()),
        Ok(meta) if !meta.is_file() => {
            return Err(CopyError::Io(std::io::Error::other(
                "destination exists but is not a regular file",
            )))
        }
        Ok(_) => {}
    }
    let source = match source_hash {
        Some(hash) => hash.clone(),
        None => source_hash.insert(hash(src, true)?).clone(),
    };
    let destination = match hash(dst, false) {
        Err(CopyError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Comparison::Missing)
        }
        other => other?,
    };
    Ok(if source == destination {
        Comparison::Match(source)
    } else {
        Comparison::Different {
            source_hash: source,
            destination_hash: destination,
        }
    })
}

/// Asked once per differing destination file.
pub type Decider<'a> = &'a mut dyn FnMut(&ConflictInfo) -> Result<ConflictDecision, Cancelled>;

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
    copy_resolving(src, dst, algo, verify, known_hash, handle, &mut |_| {
        Ok(ConflictDecision::KeepBoth)
    })
}

/// Like [`copy_verified`], but a differing destination file is resolved by `decide`.
/// New targets are claimed without clobbering files that appear meanwhile, and replacing
/// only happens after the staged copy is verified and the old file is unchanged.
pub fn copy_resolving(
    src: &Path,
    dst: &Path,
    algo: HashAlgo,
    verify: VerifyMode,
    known_hash: Option<&str>,
    handle: &JobHandle,
    decide: Decider<'_>,
) -> Result<CopyOutcome, CopyError> {
    let partial = partial_path(dst);
    let _phase = handle.analysis_phase(AnalysisPhase::Other);
    let result = resolve_and_copy(src, dst, &partial, algo, verify, known_hash, handle, decide);
    if let Ok(outcome) = &result {
        handle.analysis_metrics(|metrics| {
            if outcome.adopted {
                metrics.adopted_files += 1;
            } else if outcome.skipped {
                metrics.skipped_files += 1;
            }
        });
    }
    // After a successful commit the partial no longer exists.
    let _ = fs::remove_file(&partial);
    result
}

#[allow(clippy::too_many_arguments)]
fn resolve_and_copy(
    src: &Path,
    dst: &Path,
    partial: &Path,
    algo: HashAlgo,
    verify: VerifyMode,
    known_hash: Option<&str>,
    handle: &JobHandle,
    decide: Decider<'_>,
) -> Result<CopyOutcome, CopyError> {
    let mut target = dst.to_path_buf();
    let mut keep_both = false;
    let mut source_hash: Option<String> = None;
    let mut staged: Option<(String, u64)> = None;
    loop {
        handle.checkpoint()?;
        if keep_both && target.exists() {
            target = free_name(dst);
            continue;
        }
        match compare_files(src, &target, algo, handle, &mut source_hash)? {
            Comparison::Match(hash) => {
                return Ok(CopyOutcome {
                    hash,
                    final_path: target,
                    adopted: true,
                    skipped: false,
                    replaced: false,
                    skip_evidence: None,
                })
            }
            Comparison::Missing => {
                let (hash, bytes) =
                    stage(src, partial, algo, verify, known_hash, handle, &mut staged)?;
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)?;
                }
                if commit_new(partial, &target)? {
                    handle.analysis_metrics(|metrics| {
                        metrics.committed_bytes += bytes;
                        metrics.transferred_files += 1;
                    });
                    return Ok(CopyOutcome {
                        hash,
                        final_path: target,
                        adopted: false,
                        skipped: false,
                        replaced: false,
                        skip_evidence: None,
                    });
                }
            }
            Comparison::Different {
                source_hash: source,
                destination_hash,
            } => {
                let info = ConflictInfo {
                    source_path: src.display().to_string(),
                    destination_path: target.display().to_string(),
                    source_hash: source.clone(),
                    destination_hash: destination_hash.clone(),
                };
                match decide(&info)? {
                    ConflictDecision::Skip => {
                        let current_source_hash = hash_checked(src, algo, handle)?;
                        let current_destination_hash = match hash_checked(&target, algo, handle) {
                            Ok(hash) => hash,
                            Err(CopyError::Io(error))
                                if error.kind() == std::io::ErrorKind::NotFound =>
                            {
                                source_hash = Some(current_source_hash);
                                continue;
                            }
                            Err(error) => return Err(error),
                        };
                        if current_source_hash != source
                            || current_destination_hash != destination_hash
                        {
                            source_hash = Some(current_source_hash);
                            continue;
                        }
                        let source_size = fs::metadata(src)?.len();
                        return Ok(CopyOutcome {
                            hash: current_source_hash.clone(),
                            final_path: target,
                            adopted: false,
                            skipped: true,
                            replaced: false,
                            skip_evidence: Some(SkipEvidence {
                                source_hash: current_source_hash,
                                source_size,
                                destination_hash,
                            }),
                        });
                    }
                    ConflictDecision::KeepBoth => {
                        keep_both = true;
                        target = free_name(dst);
                        continue;
                    }
                    ConflictDecision::Replace => {}
                }
                let (hash, bytes) =
                    stage(src, partial, algo, verify, known_hash, handle, &mut staged)?;
                // The old file may have changed while the copy was staged or the user decided.
                match hash_checked(&target, algo, handle) {
                    Ok(now) if now == destination_hash => {
                        fs::rename(partial, &target)?;
                        handle.analysis_metrics(|metrics| {
                            metrics.committed_bytes += bytes;
                            metrics.transferred_files += 1;
                        });
                        return Ok(CopyOutcome {
                            hash,
                            final_path: target,
                            adopted: false,
                            skipped: false,
                            replaced: true,
                            skip_evidence: None,
                        });
                    }
                    Err(CopyError::Cancelled) => return Err(CopyError::Cancelled),
                    // Changed while staging: look at the new content and ask again.
                    _ => {}
                }
            }
        }
    }
}

/// Copies and verifies the source into the partial file once; later attempts reuse it.
fn stage(
    src: &Path,
    partial: &Path,
    algo: HashAlgo,
    verify: VerifyMode,
    known_hash: Option<&str>,
    handle: &JobHandle,
    staged: &mut Option<(String, u64)>,
) -> Result<(String, u64), CopyError> {
    if let Some(hash) = staged {
        return Ok(hash.clone());
    }
    if let Some(parent) = partial.parent() {
        fs::create_dir_all(parent)?;
    }
    let (hash, bytes) = stream_copy(src, partial, algo, handle)?;
    if known_hash.is_some_and(|k| k != hash) {
        return Err(CopyError::SourceChanged);
    }
    if let Ok(modified) = fs::metadata(src).and_then(|m| m.modified()) {
        let _ = File::options()
            .write(true)
            .open(partial)
            .and_then(|f| f.set_modified(modified));
    }
    if verify == VerifyMode::Reread {
        let actual = hash_checked(partial, algo, handle)?;
        if actual != hash {
            return Err(CopyError::HashMismatch {
                expected: hash,
                actual,
            });
        }
    }
    *staged = Some((hash.clone(), bytes));
    Ok((hash, bytes))
}

/// Moves the staged file to a target that must not exist. `Ok(false)` means somebody
/// created the target first and nothing was overwritten.
fn commit_new(partial: &Path, target: &Path) -> Result<bool, CopyError> {
    match fs::hard_link(partial, target) {
        Ok(()) => {
            let _ = fs::remove_file(partial);
            return Ok(true);
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => return Ok(false),
        // Some filesystems (exFAT, FAT) have no hard links: claim the name instead.
        Err(_) => {}
    }
    match OpenOptions::new().write(true).create_new(true).open(target) {
        Ok(claimed) => {
            drop(claimed);
            if let Err(e) = fs::rename(partial, target) {
                let _ = fs::remove_file(target);
                return Err(e.into());
            }
            Ok(true)
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
        Err(e) => Err(e.into()),
    }
}

fn stream_copy(
    src: &Path,
    partial: &Path,
    algo: HashAlgo,
    handle: &JobHandle,
) -> Result<(String, u64), CopyError> {
    let _phase = handle.analysis_phase(AnalysisPhase::Copy);
    let mut input = File::open(src)?;
    let mut output = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(partial)?;
    let mut state = hasher(algo);
    let mut buf = vec![0u8; BUFFER_SIZE];
    let mut copied = 0;
    loop {
        handle.checkpoint()?;
        let n = input.read(&mut buf)?;
        if n == 0 {
            break;
        }
        state.update(&buf[..n]);
        copied += write_counted(&mut output, &buf[..n], handle)?;
        handle.add_bytes(n as u64);
    }
    output.sync_all()?;
    Ok((state.finish_hex(), copied))
}

fn write_counted(
    output: &mut impl Write,
    bytes: &[u8],
    handle: &JobHandle,
) -> std::io::Result<u64> {
    let mut written = 0;
    while written < bytes.len() {
        match output.write(&bytes[written..]) {
            Ok(0) => return Err(std::io::ErrorKind::WriteZero.into()),
            Ok(count) => {
                written += count;
                handle.analysis_metrics(|metrics| metrics.copy_bytes += count as u64);
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(written as u64)
}

pub(super) fn hash_checked(
    path: &Path,
    algo: HashAlgo,
    handle: &JobHandle,
) -> Result<String, CopyError> {
    hash_checked_side(path, algo, handle, false)
}

fn hash_checked_side(
    path: &Path,
    algo: HashAlgo,
    handle: &JobHandle,
    source: bool,
) -> Result<String, CopyError> {
    hash_checked_reporting(path, algo, handle, source, |_| {})
}

pub(super) fn hash_checked_with_progress(
    path: &Path,
    algo: HashAlgo,
    handle: &JobHandle,
    source: bool,
    mut on_bytes: impl FnMut(u64, u64),
) -> Result<String, CopyError> {
    handle.checkpoint()?;
    let total = fs::metadata(path)?.len();
    hash_checked_reporting(path, algo, handle, source, |bytes| on_bytes(bytes, total))
}

fn hash_checked_reporting(
    path: &Path,
    algo: HashAlgo,
    handle: &JobHandle,
    source: bool,
    mut on_bytes: impl FnMut(u64),
) -> Result<String, CopyError> {
    handle.checkpoint()?;
    let _phase = handle.analysis_phase(if source {
        AnalysisPhase::SourceCheck
    } else {
        AnalysisPhase::DestinationCheck
    });
    hash_file(path, algo, |bytes| {
        // The callback is after the read: count it even if this checkpoint cancels.
        handle.analysis_metrics(|metrics| {
            if source {
                metrics.source_check_bytes += bytes;
            } else {
                metrics.destination_check_bytes += bytes;
            }
        });
        if handle.checkpoint().is_err() {
            return false;
        }
        on_bytes(bytes);
        // A progress observer may cancel or pause the job after this chunk.
        handle.checkpoint().is_ok()
    })
    .map_err(|e| {
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
    let stem = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let ext = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    (1..)
        .map(|n| path.with_file_name(format!("{stem} ({n}){ext}")))
        .find(|p| !p.exists())
        .expect("unbounded range")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transfer::handle::{JobKind, TransferJob};
    use std::cell::Cell;
    use std::sync::Arc;

    fn handle() -> JobHandle {
        JobHandle::new(
            TransferJob::new("j".into(), "f".into(), String::new(), JobKind::Transfer),
            Arc::new(|| {}),
        )
    }

    #[test]
    fn comparison_and_reread_hashing_do_not_change_transfer_byte_accounting() {
        let size = 3 * BUFFER_SIZE;
        let (dir, src, dst) = files(&vec![1; size], &vec![1; size]);
        let handle = handle();
        let comparison = compare_files(&src, &dst, HashAlgo::Blake3, &handle, &mut None).unwrap();
        assert!(matches!(comparison, Comparison::Match(_)));
        assert_eq!(handle.snapshot().bytes_done, 0);
        let copied = copy_verified(
            &src,
            &dir.path().join("new.jpg"),
            HashAlgo::Blake3,
            VerifyMode::Reread,
            None,
            &handle,
        )
        .unwrap();
        assert!(!copied.adopted);
        assert_eq!(handle.snapshot().bytes_done, size as u64);
    }

    fn files(src: &[u8], dst: &[u8]) -> (tempfile::TempDir, PathBuf, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let (s, d) = (dir.path().join("src.jpg"), dir.path().join("out/dst.jpg"));
        fs::create_dir_all(d.parent().unwrap()).unwrap();
        fs::write(&s, src).unwrap();
        fs::write(&d, dst).unwrap();
        (dir, s, d)
    }

    fn copy(
        s: &Path,
        d: &Path,
        known: Option<&str>,
        mut decide: impl FnMut(&ConflictInfo) -> ConflictDecision,
    ) -> Result<CopyOutcome, CopyError> {
        copy_resolving(
            s,
            d,
            HashAlgo::Xxh64,
            VerifyMode::Reread,
            known,
            &handle(),
            &mut |i| Ok(decide(i)),
        )
    }

    fn partials(d: &Path) -> bool {
        d.parent()
            .unwrap()
            .read_dir()
            .unwrap()
            .any(|e| e.unwrap().file_name().to_string_lossy().contains("partial"))
    }

    #[test]
    fn skip_leaves_destination_untouched() {
        let (_t, s, d) = files(b"new", b"old");
        let o = copy(&s, &d, None, |_| ConflictDecision::Skip).unwrap();
        assert!(o.skipped && !o.adopted);
        assert_eq!(fs::read(&d).unwrap(), b"old");
        assert!(!partials(&d));
    }

    #[test]
    fn keep_both_uses_numbered_name() {
        let (_t, s, d) = files(b"new", b"old");
        let o = copy(&s, &d, None, |_| ConflictDecision::KeepBoth).unwrap();
        assert_eq!(o.final_path, d.with_file_name("dst (1).jpg"));
        assert_eq!(fs::read(&d).unwrap(), b"old");
        assert_eq!(fs::read(&o.final_path).unwrap(), b"new");
    }

    #[test]
    fn replace_commits_verified_staged_copy() {
        let (_t, s, d) = files(b"new", b"old");
        let o = copy(&s, &d, None, |i| {
            assert_ne!(i.source_hash, i.destination_hash);
            ConflictDecision::Replace
        })
        .unwrap();
        assert!(o.replaced);
        assert_eq!(fs::read(&d).unwrap(), b"new");
        assert!(!partials(&d));
    }

    #[test]
    fn stale_known_hash_is_not_trusted_and_failed_replace_keeps_destination() {
        let (_t, s, d) = files(b"edited-same-size!", b"original-content!");
        let stale = hash_file(&d, HashAlgo::Xxh64, |_| true).unwrap();
        let asked = Cell::new(0);
        let err = copy(&s, &d, Some(&stale), |_| {
            asked.set(asked.get() + 1);
            ConflictDecision::Replace
        })
        .unwrap_err();
        assert!(matches!(err, CopyError::SourceChanged));
        assert_eq!(asked.get(), 1);
        assert_eq!(fs::read(&d).unwrap(), b"original-content!");
        assert!(!partials(&d));
    }

    #[test]
    fn replace_rechecks_destination_changed_while_waiting() {
        let (_t, s, d) = files(b"new", b"old");
        let asked = Cell::new(0);
        let o = copy(&s, &d, None, |i| {
            asked.set(asked.get() + 1);
            if asked.get() == 1 {
                fs::write(&d, b"changed meanwhile").unwrap();
                ConflictDecision::Replace
            } else {
                assert_eq!(
                    i.destination_hash,
                    hash_file(&d, HashAlgo::Xxh64, |_| true).unwrap()
                );
                ConflictDecision::Skip
            }
        })
        .unwrap();
        assert!(o.skipped);
        assert_eq!(asked.get(), 2);
        assert_eq!(fs::read(&d).unwrap(), b"changed meanwhile");
    }

    #[test]
    fn target_appearing_while_copying_is_adopted_or_asked_not_clobbered() {
        let (_t, s, d) = files(b"new", b"x");
        fs::remove_file(&d).unwrap();
        let partial = partial_path(&d);
        fs::write(&partial, b"new").unwrap();
        fs::write(&d, b"someone else").unwrap();
        assert!(!commit_new(&partial, &d).unwrap());
        assert_eq!(fs::read(&d).unwrap(), b"someone else");
        assert!(partial.exists());
        let o = copy(&s, &d, None, |_| ConflictDecision::KeepBoth).unwrap();
        assert_eq!(fs::read(&o.final_path).unwrap(), b"new");
        assert_eq!(fs::read(&d).unwrap(), b"someone else");
    }

    #[test]
    fn non_file_destination_is_an_error() {
        let (_t, s, d) = files(b"new", b"old");
        fs::remove_file(&d).unwrap();
        fs::create_dir(&d).unwrap();
        assert!(copy(&s, &d, None, |_| ConflictDecision::Replace).is_err());
        assert!(d.is_dir());
    }

    #[test]
    fn analysis_short_writes_count_actual_io_including_failed_attempts() {
        struct ShortWriter {
            written: usize,
            fail_after: Option<usize>,
            interrupted: bool,
        }
        impl Write for ShortWriter {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                if self.interrupted {
                    self.interrupted = false;
                    return Err(std::io::ErrorKind::Interrupted.into());
                }
                if self.fail_after.is_some_and(|limit| self.written >= limit) {
                    return Err(std::io::ErrorKind::BrokenPipe.into());
                }
                let count = bytes.len().min(3);
                self.written += count;
                Ok(count)
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let h = handle().with_analysis(Some(crate::transfer::metrics::tests::context()), None);
        h.start_work();
        let mut failed = ShortWriter {
            written: 0,
            fail_after: Some(3),
            interrupted: true,
        };
        assert!(write_counted(&mut failed, b"0123456789", &h).is_err());
        assert_eq!(h.snapshot().analysis.unwrap().metrics.copy_bytes, 3);
        let mut retry = ShortWriter {
            written: 0,
            fail_after: None,
            interrupted: true,
        };
        assert_eq!(write_counted(&mut retry, b"0123456789", &h).unwrap(), 10);
        let snapshot = h.snapshot();
        assert_eq!(snapshot.analysis.unwrap().metrics.copy_bytes, 13);
        assert_eq!(snapshot.bytes_done, 0);
    }
}
