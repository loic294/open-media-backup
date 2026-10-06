//! Read-only destination inventory and catalog verification.
//!
//! The scan covers the static directory prefix of the destination template,
//! including all project/source subfolders, even when their sources are offline.
//! `photo/{project}/...` scans `photo`; a template starting with a variable scans
//! the device root. Rules and source filters do not restrict this inventory.
//! Verified means comparable catalog evidence, not a fresh source match.
//! Untracked files are inventoried without creating content or copy claims.
//! Symlinks (including directory links), special files and inaccessible entries
//! produce errors, never missing/verified claims. No media or markers are written.

use super::check::HashProgress;
use super::copy::CopyError;
use super::destination_hasher::DestinationHasher;
use super::handle::{CheckItem, CheckOutcome, CheckResults, JobHandle, JobState};
use super::job::RecordWriter;
use crate::domain::{Destination, DestinationKind, Device, FileCopy, HashAlgo, SafeCopyOverride};
use crate::paths::{expand, join_relative, to_relative, TemplateVars};
use crate::plan::{Catalog, RootResolver, WorkspaceContext};
use crate::store::Store;
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

pub(crate) struct DestinationCheck {
    pub device: Device,
    device_root: PathBuf,
    folder: String,
    hasher: DestinationHasher,
}

pub(crate) fn prepare_destination_check(
    store: &Store,
    resolver: &dyn RootResolver,
    context: &WorkspaceContext,
    destination: &Destination,
) -> Result<DestinationCheck, String> {
    context.load(store).map_err(|e| e.to_string())?;
    if destination.space_id != context.space_id || destination.kind != DestinationKind::Folder {
        return Err("Check requires a folder destination in this workspace".into());
    }
    let device: Device = store
        .get(&destination.device_id)
        .map_err(|e| e.to_string())?
        .ok_or("Select a destination device before checking")?;
    let device_root = resolver
        .device_root(&device.id)
        .filter(|root| root.is_dir())
        .ok_or("Connect the destination device to check")?
        .canonicalize()
        .map_err(|e| e.to_string())?;
    // Validate template syntax without requiring source/project values.
    if matches!(
        expand(&destination.path_template, &TemplateVars::new()),
        Err(crate::paths::TemplateError::Unclosed(_))
    ) {
        return Err("Destination path has an unclosed template variable".into());
    }
    let mut segments = Vec::new();
    for segment in destination.path_template.split(['/', '\\']) {
        if segment.contains('{') {
            break;
        }
        if segment == ".." || segment.contains(':') {
            return Err("Destination scan path must stay inside its device root".into());
        }
        if !segment.is_empty() && segment != "." {
            segments.push(segment);
        }
    }
    let folder = segments.join("/");
    let hasher = DestinationHasher::resolve(store, destination.remote_hash.as_ref(), &device_root)?;
    Ok(DestinationCheck {
        device,
        device_root,
        folder,
        hasher,
    })
}

/// Each component is inspected without following symlinks. Missing is returned
/// only after all existing ancestors were proved safe and accessible.
fn safe_metadata(root: &Path, relative: &str) -> io::Result<Option<fs::Metadata>> {
    let path = Path::new(relative);
    if relative.contains(['\\', '\0', ':'])
        || path.is_absolute()
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "unsafe catalog path",
        ));
    }
    let mut current = root.to_path_buf();
    let mut metadata = fs::symlink_metadata(root)?;
    for part in path.components() {
        current.push(part);
        metadata = match fs::symlink_metadata(&current) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        if metadata.file_type().is_symlink() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "symlinks are not checked",
            ));
        }
    }
    Ok(Some(metadata))
}

fn item(path: &str, outcome: CheckOutcome, error: Option<String>) -> CheckItem {
    CheckItem {
        source_path: String::new(),
        destination_path: path.into(),
        outcome,
        error,
    }
}

pub(crate) fn run_destination_check(
    store: &Store,
    resolver: &dyn RootResolver,
    context: &WorkspaceContext,
    destination: &Destination,
    handle: &JobHandle,
) -> Result<(), String> {
    let ctx = prepare_destination_check(store, resolver, context, destination)?;
    let catalog = Catalog::load(store).map_err(|e| e.to_string())?;
    let copies: Vec<FileCopy> = store
        .list::<FileCopy>()
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter(|copy| {
            !copy.removed && copy.device_id == ctx.device.id && under(&copy.path, &ctx.folder)
        })
        .collect();
    let overrides: Vec<SafeCopyOverride> = store
        .list::<SafeCopyOverride>()
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter(|claim| {
            claim.destination_device_id == ctx.device.id
                && under(&claim.destination_path, &ctx.folder)
        })
        .collect();
    // Union filesystem entries with claims, so removed files also get results.
    let mut paths: BTreeMap<String, u64> = copies
        .iter()
        .map(|claim| (claim.path.clone(), 0))
        .chain(
            overrides
                .iter()
                .map(|claim| (claim.destination_path.clone(), 0)),
        )
        .collect();
    let mut enumeration_errors = BTreeMap::new();
    match safe_metadata(&ctx.device_root, &ctx.folder) {
        Ok(Some(metadata)) if metadata.is_dir() => {
            for entry in walkdir::WalkDir::new(join_relative(&ctx.device_root, &ctx.folder))
                .follow_links(false)
                .follow_root_links(false)
            {
                if handle.checkpoint().is_err() {
                    break;
                }
                match entry {
                    Ok(entry) => {
                        if entry.file_type().is_dir() {
                            continue;
                        }
                        let rel = to_relative(&ctx.device_root, entry.path())
                            .ok_or("entry outside destination")?;
                        match safe_metadata(&ctx.device_root, &rel) {
                            Ok(Some(metadata)) if metadata.is_file() => {
                                paths.insert(rel, metadata.len());
                            }
                            Ok(_) => {
                                enumeration_errors
                                    .insert(rel, "Not a readable regular file".into());
                            }
                            Err(error) => {
                                enumeration_errors.insert(rel, error.to_string());
                            }
                        }
                    }
                    Err(error) => {
                        let rel = error
                            .path()
                            .and_then(|path| to_relative(&ctx.device_root, path))
                            .unwrap_or_else(|| ctx.folder.clone());
                        enumeration_errors.insert(rel, error.to_string());
                    }
                }
            }
        }
        Ok(None) => {}
        Ok(Some(_)) => {
            enumeration_errors.insert(
                ctx.folder.clone(),
                "Destination scan root is not a directory".into(),
            );
        }
        Err(error) => {
            enumeration_errors.insert(ctx.folder.clone(), error.to_string());
        }
    }
    for path in enumeration_errors.keys() {
        paths.entry(path.clone()).or_default();
    }
    let mut copies_by_path: BTreeMap<&str, Vec<&FileCopy>> = BTreeMap::new();
    for copy in &copies {
        copies_by_path.entry(&copy.path).or_default().push(copy);
    }
    let mut overrides_by_path: BTreeMap<&str, Vec<&SafeCopyOverride>> = BTreeMap::new();
    for claim in &overrides {
        overrides_by_path
            .entry(&claim.destination_path)
            .or_default()
            .push(claim);
    }
    handle.update(|job| {
        job.remote_hash_active = ctx.hasher.is_remote();
        job.files_total = paths.len();
        job.bytes_total = paths.values().sum();
        if job.state != JobState::Paused {
            job.state = JobState::Running;
        }
    });
    let mut results = CheckResults::default();
    let mut writer = RecordWriter::new(store, &catalog);
    for (path, size) in paths {
        if handle.checkpoint().is_err() {
            break;
        }
        let before = handle.snapshot().bytes_done;
        handle.update(|job| job.current_file = Some(path.clone()));
        let result = if let Some(error) = enumeration_errors.get(&path) {
            Ok(item(&path, CheckOutcome::Error, Some(error.clone())))
        } else {
            verify_path(
                &ctx,
                &catalog,
                copies_by_path
                    .get(path.as_str())
                    .map(Vec::as_slice)
                    .unwrap_or_default(),
                overrides_by_path
                    .get(path.as_str())
                    .map(Vec::as_slice)
                    .unwrap_or_default(),
                &path,
                handle,
                &mut writer,
            )
        };
        match result {
            Err(CopyError::Cancelled) => break,
            Err(error) => {
                handle.analysis_error();
                results.add(item(&path, CheckOutcome::Error, Some(error.to_string())));
            }
            Ok(result) => {
                if result.outcome == CheckOutcome::Error {
                    handle.analysis_error();
                }
                results.add(result);
            }
        }
        handle.update(|job| {
            job.files_done += 1;
            job.bytes_done = before + size;
        });
        if writer.len() >= 50 {
            writer.flush().map_err(|e| e.to_string())?;
        }
    }
    writer.flush().map_err(|e| e.to_string())?;
    handle.update(|job| {
        job.current_file = None;
        job.check_results = Some(results);
    });
    Ok(())
}

fn under(path: &str, folder: &str) -> bool {
    folder.is_empty() || path.starts_with(&format!("{folder}/")) || path == folder
}

fn verify_path(
    ctx: &DestinationCheck,
    catalog: &Catalog,
    copies: &[&FileCopy],
    overrides: &[&SafeCopyOverride],
    path: &str,
    handle: &JobHandle,
    writer: &mut RecordWriter<'_>,
) -> Result<CheckItem, CopyError> {
    let Some(metadata) = safe_metadata(&ctx.device_root, path)? else {
        for copy in copies {
            writer.invalidate_copy_only(copy);
        }
        for claim in overrides {
            writer.invalidate_override(claim);
        }
        return Ok(item(path, CheckOutcome::Missing, None));
    };
    if !metadata.is_file() {
        return Ok(item(
            path,
            CheckOutcome::Error,
            Some("Not a readable regular file".into()),
        ));
    }
    let mut evidence: Vec<(HashAlgo, &str)> = Vec::new();
    for copy in copies {
        if let Some(record) = catalog.record(&copy.file_id) {
            evidence.push((record.hash_algo, &record.hash));
        }
    }
    for claim in overrides {
        if let Some(record) = catalog.record(&claim.file_id) {
            evidence.push((record.hash_algo, &claim.destination_hash));
        }
    }
    // A missing/corrupt catalog record is not evidence of either match or change.
    evidence.retain(|(algo, hash)| valid_digest(*algo, hash));
    if evidence.is_empty() {
        // Inventory is not a hash comparison, but unreadable files still need
        // attention rather than being silently reported as ordinary untracked files.
        fs::File::open(join_relative(&ctx.device_root, path))?;
    }
    let mut algorithms = Vec::new();
    for (algo, _) in &evidence {
        if !algorithms.contains(algo) {
            algorithms.push(*algo);
        }
    }
    let mut hashes = Vec::new();
    let before = handle.snapshot().bytes_done;
    for (index, algo) in algorithms.iter().enumerate() {
        let start = metadata.len() * index as u64 / algorithms.len() as u64;
        let end = metadata.len() * (index + 1) as u64 / algorithms.len() as u64;
        let mut progress = HashProgress::new(handle, before + start, end - start);
        safe_metadata(&ctx.device_root, path)?.ok_or(CopyError::SourceChanged)?;
        let hash = ctx.hasher.hash(
            &join_relative(&ctx.device_root, path),
            *algo,
            handle,
            |bytes, total| progress.add(bytes, total),
        )?;
        safe_metadata(&ctx.device_root, path)?.ok_or(CopyError::SourceChanged)?;
        hashes.push((*algo, hash));
    }
    let different = |algo, expected: &str| {
        valid_digest(algo, expected)
            && hashes
                .iter()
                .find(|(hashed_algo, _)| *hashed_algo == algo)
                .is_some_and(|(_, actual)| actual != expected)
    };
    let changed = evidence
        .iter()
        .any(|(algo, expected)| different(*algo, expected));
    if !evidence.is_empty() {
        let after = safe_metadata(&ctx.device_root, path)?.ok_or(CopyError::SourceChanged)?;
        if !after.is_file()
            || metadata.len() != after.len()
            || metadata.modified()? != after.modified()?
        {
            return Err(CopyError::SourceChanged);
        }
    }
    for copy in copies {
        if catalog
            .record(&copy.file_id)
            .is_some_and(|record| different(record.hash_algo, &record.hash))
        {
            writer.invalidate_copy_only(copy);
        }
    }
    for claim in overrides {
        if catalog
            .record(&claim.file_id)
            .is_some_and(|record| different(record.hash_algo, &claim.destination_hash))
        {
            writer.invalidate_override(claim);
        }
    }
    Ok(item(
        path,
        if changed {
            CheckOutcome::Conflict
        } else if evidence.is_empty() {
            CheckOutcome::Untracked
        } else {
            CheckOutcome::Verified
        },
        None,
    ))
}

fn valid_digest(algo: HashAlgo, hash: &str) -> bool {
    hash.len()
        == match algo {
            HashAlgo::Blake3 => 64,
            HashAlgo::Xxh64 => 16,
        }
        && hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
