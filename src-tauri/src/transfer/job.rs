use super::copy::{copy_resolving_with_hasher, CopyError, SkipEvidence};
use super::handle::{JobHandle, JobState};
use crate::app::AppSettings;
use crate::domain::{FileCopy, FileRecord, SafeCopyOverride};
use crate::paths::{ensure_backup_folder, join_relative, to_relative};
use crate::plan::{
    classify_flow, resolve_workspace_flow, Catalog, Category, FailureMap, FlowContext, PlannedFile,
    RootResolver, WorkspaceContext,
};
use crate::store::Store;
use parking_lot::Mutex;
use std::collections::HashSet;

const BATCH: usize = 50;

/// Copies every pending file of a flow, recording the verified copies in the catalog.
pub fn run_transfer(
    store: &Store,
    resolver: &dyn RootResolver,
    project_id: &str,
    flow_id: &str,
    handle: &JobHandle,
    failures: &Mutex<FailureMap>,
) -> Result<(), String> {
    let context = WorkspaceContext::for_project(store, project_id).map_err(|e| e.to_string())?;
    run_workspace_transfer(store, resolver, &context, flow_id, handle, failures)
}

pub fn run_workspace_transfer(
    store: &Store,
    resolver: &dyn RootResolver,
    context: &WorkspaceContext,
    flow_id: &str,
    handle: &JobHandle,
    failures: &Mutex<FailureMap>,
) -> Result<(), String> {
    let ctx = prepare(store, resolver, context, flow_id, true)?;
    let catalog = Catalog::load(store).map_err(|e| e.to_string())?;
    let planned = classify_flow(&ctx, &catalog, None);
    let planning_errors: Vec<_> = planned
        .iter()
        .filter_map(|file| {
            file.error
                .as_ref()
                .map(|error| format!("{}: {error}", file.rel_path))
        })
        .collect();
    if !planning_errors.is_empty() {
        return Err(planning_errors.join("; "));
    }
    let pending: Vec<PlannedFile> = planned
        .into_iter()
        .filter(|f| f.category == Category::ToTransfer)
        .collect();
    handle.update(|j| {
        j.label = ctx.label();
        j.files_total = pending.len();
        j.bytes_total = pending.iter().map(|f| f.size).sum();
        if j.state != JobState::Paused {
            j.state = JobState::Running;
        }
    });

    let mut writer = RecordWriter::new(store, &catalog);
    let mut flow_failures = failures.lock().remove(flow_id).unwrap_or_default();
    flow_failures.retain(|key, _| {
        pending
            .iter()
            .any(|f| f.failure_key() == *key || f.rel_path == *key)
    });
    let mut skipped_any = false;
    for file in &pending {
        if handle.checkpoint().is_err() {
            break;
        }
        handle.update(|j| j.current_file = Some(file.rel_path.clone()));
        match transfer_one(&ctx, file, handle) {
            Ok(TransferOneOutcome::Copied {
                hash,
                destination_path,
            }) => {
                flow_failures.remove(&file.rel_path);
                flow_failures.remove(&file.failure_key());
                writer.add(&ctx, file, hash, destination_path);
            }
            // Skipped by the user: stays pending and is not a failure.
            Ok(TransferOneOutcome::Skipped {
                evidence: Some(evidence),
            }) => {
                skipped_any = true;
                if ctx.space.skip_counts_as_safe_copy && ctx.destination.counts_as_safe_copy {
                    let target = file.target_path.as_deref().expect("transfer target");
                    writer.add_skip_safe_copy(&ctx, file, target, evidence);
                }
            }
            Ok(TransferOneOutcome::Skipped { evidence: None }) => skipped_any = true,
            Err(CopyError::Cancelled) => break,
            Err(e) => {
                let message = e.to_string();
                handle.update(|j| j.errors.push(format!("{}: {message}", file.rel_path)));
                flow_failures.insert(file.failure_key(), message);
            }
        }
        handle.update(|j| j.files_done += 1);
        if writer.len() >= BATCH {
            writer.flush().map_err(|e| e.to_string())?;
        }
    }
    writer.flush().map_err(|e| e.to_string())?;
    let completed_without_errors = flow_failures.is_empty() && handle.snapshot().errors.is_empty();
    if !flow_failures.is_empty() {
        failures.lock().insert(flow_id.to_string(), flow_failures);
    }
    // Skipped files were never copied, so the byte totals would overstate throughput.
    if completed_without_errors && !skipped_any {
        record_learned_speed(store, &ctx.dest_device.id, handle);
    }
    handle.update(|j| j.current_file = None);
    Ok(())
}

fn record_learned_speed(store: &Store, destination_device_id: &str, handle: &JobHandle) {
    let job = handle.snapshot();
    let Some(elapsed) = handle.elapsed() else {
        return;
    };
    let mut settings = AppSettings::load(store);
    if settings.record_transfer_sample(
        destination_device_id,
        job.bytes_total,
        elapsed.as_secs_f64(),
    ) {
        let _ = settings.save(store);
    }
}

/// Resolves the flow and verifies both devices are usable. `create_marker` is false for
/// read-only checks, which must not write to the source.
pub(super) fn prepare(
    store: &Store,
    resolver: &dyn RootResolver,
    context: &WorkspaceContext,
    flow_id: &str,
    create_marker: bool,
) -> Result<FlowContext, String> {
    let mut ctx =
        resolve_workspace_flow(store, resolver, context, flow_id).map_err(|e| e.to_string())?;
    if ctx.source_root.is_none() {
        return Err(format!("{} is not connected", ctx.source_device.name));
    }
    if ctx.dest_root.is_none() {
        return Err(format!("{} is not connected", ctx.dest_device.name));
    }
    if let Some(err) = &ctx.config_error {
        return Err(err.clone());
    }
    if create_marker && ctx.destination.use_backup_marker {
        let root = ctx.source_root.clone().expect("checked above");
        let folder = ctx.vars.get("backup_folder").cloned().unwrap_or_default();
        ensure_backup_folder(&root, &folder).map_err(|e| format!("backup marker: {e}"))?;
        ctx =
            resolve_workspace_flow(store, resolver, context, flow_id).map_err(|e| e.to_string())?;
    }
    ctx.destination_hasher = super::destination_hasher::DestinationHasher::resolve(
        store,
        ctx.destination.remote_hash.as_ref(),
        ctx.dest_root.as_deref().expect("checked destination root"),
    )?;
    Ok(ctx)
}

enum TransferOneOutcome {
    Copied {
        hash: String,
        destination_path: String,
    },
    Skipped {
        evidence: Option<SkipEvidence>,
    },
}

fn transfer_one(
    ctx: &FlowContext,
    file: &PlannedFile,
    handle: &JobHandle,
) -> Result<TransferOneOutcome, CopyError> {
    let src = file.abs_path.as_ref().ok_or(CopyError::SourceChanged)?;
    let root = ctx.dest_root.as_ref().ok_or(CopyError::SourceChanged)?;
    let target = file
        .target_path
        .as_deref()
        .ok_or(CopyError::SourceChanged)?;
    let dst = join_relative(root, target);
    // The catalog hash may predate an edit of the source, so the copy's own fresh hash
    // becomes the source identity instead of being checked against it.
    let before = handle.snapshot().bytes_done;
    handle.update(|j| j.remote_hash_active = ctx.destination_hasher.is_remote());
    let outcome = copy_resolving_with_hasher(
        src,
        &dst,
        ctx.space.hash_algo,
        ctx.space.verify_mode,
        None,
        handle,
        &ctx.destination_hasher,
        &mut |info| handle.request_decision(info),
    );
    // Keep the byte counter aligned with the file size, whatever was re-read or skipped.
    handle.update(|j| j.bytes_done = before + file.size);
    let outcome = outcome?;
    if outcome.skipped {
        return Ok(TransferOneOutcome::Skipped {
            evidence: outcome.skip_evidence,
        });
    }
    let dest_rel = to_relative(root, &outcome.final_path).ok_or_else(|| {
        CopyError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Transferred target is outside destination root",
        ))
    })?;
    Ok(TransferOneOutcome::Copied {
        hash: outcome.hash,
        destination_path: dest_rel,
    })
}

pub(super) struct RecordWriter<'a> {
    store: &'a Store,
    catalog: &'a Catalog,
    written: HashSet<String>,
    removed: HashSet<String>,
    removed_overrides: HashSet<String>,
    records: Vec<FileRecord>,
    copies: Vec<FileCopy>,
    overrides: Vec<SafeCopyOverride>,
}

impl<'a> RecordWriter<'a> {
    pub(super) fn new(store: &'a Store, catalog: &'a Catalog) -> Self {
        Self {
            store,
            catalog,
            written: HashSet::new(),
            removed: HashSet::new(),
            removed_overrides: HashSet::new(),
            records: vec![],
            copies: vec![],
            overrides: vec![],
        }
    }

    pub(super) fn len(&self) -> usize {
        self.copies.len()
    }

    pub(super) fn add(
        &mut self,
        ctx: &FlowContext,
        file: &PlannedFile,
        hash: String,
        dest_rel: String,
    ) {
        let algo = ctx.space.hash_algo;
        let file_id = FileRecord::id_for(algo, &hash);
        let source_path = ctx.source_device_path(&file.rel_path);
        if self.catalog.record(&file_id).is_none() && self.written.insert(file_id.clone()) {
            self.records.push(FileRecord {
                id: file_id.clone(),
                hash,
                hash_algo: algo,
                size: file.size,
                name: file
                    .rel_path
                    .rsplit('/')
                    .next()
                    .unwrap_or_default()
                    .to_string(),
                origin_device_id: ctx.source_device.id.clone(),
                origin_path: source_path.clone(),
                modified_at: file.modified_ms,
            });
        }
        let now = chrono::Utc::now().timestamp_millis();
        self.invalidate_other_claims(&ctx.source_device.id, &source_path, &file_id);
        self.invalidate_source_overrides(&ctx.source_device.id, &source_path, &file_id);
        self.copies.push(FileCopy {
            id: FileCopy::id_for(&file_id, &ctx.source_device.id, &source_path),
            file_id: file_id.clone(),
            device_id: ctx.source_device.id.clone(),
            path: source_path,
            verified_at: now,
            removed: false,
        });
        self.invalidate_other_claims(&ctx.dest_device.id, &dest_rel, &file_id);
        self.invalidate_destination_overrides(&ctx.dest_device.id, &dest_rel);
        self.copies.push(FileCopy {
            id: FileCopy::id_for(&file_id, &ctx.dest_device.id, &dest_rel),
            file_id: file_id.clone(),
            device_id: ctx.dest_device.id.clone(),
            path: dest_rel,
            verified_at: now,
            removed: false,
        });
    }

    pub(super) fn add_skip_safe_copy(
        &mut self,
        ctx: &FlowContext,
        file: &PlannedFile,
        destination_path: &str,
        evidence: SkipEvidence,
    ) {
        let file_id = FileRecord::id_for(ctx.space.hash_algo, &evidence.source_hash);
        let source_path = ctx.source_device_path(&file.rel_path);
        if self.catalog.record(&file_id).is_none() && self.written.insert(file_id.clone()) {
            self.records.push(FileRecord {
                id: file_id.clone(),
                hash: evidence.source_hash,
                hash_algo: ctx.space.hash_algo,
                size: evidence.source_size,
                name: file
                    .rel_path
                    .rsplit('/')
                    .next()
                    .unwrap_or_default()
                    .to_string(),
                origin_device_id: ctx.source_device.id.clone(),
                origin_path: source_path.clone(),
                modified_at: file.modified_ms,
            });
        }
        self.invalidate_other_claims(&ctx.source_device.id, &source_path, &file_id);
        self.invalidate_source_overrides(&ctx.source_device.id, &source_path, &file_id);
        self.copies.push(FileCopy {
            id: FileCopy::id_for(&file_id, &ctx.source_device.id, &source_path),
            file_id: file_id.clone(),
            device_id: ctx.source_device.id.clone(),
            path: source_path.clone(),
            verified_at: chrono::Utc::now().timestamp_millis(),
            removed: false,
        });
        self.invalidate_other_claims(&ctx.dest_device.id, destination_path, &file_id);
        self.invalidate_destination_overrides(&ctx.dest_device.id, destination_path);
        self.overrides.push(SafeCopyOverride {
            id: SafeCopyOverride::id_for(
                &ctx.space.id,
                &file_id,
                &ctx.dest_device.id,
                destination_path,
            ),
            space_id: ctx.space.id.clone(),
            file_id,
            source_device_id: ctx.source_device.id.clone(),
            source_path,
            destination_device_id: ctx.dest_device.id.clone(),
            destination_path: destination_path.to_string(),
            destination_hash: evidence.destination_hash,
        });
    }

    /// A location now holding `file_id` can no longer be claimed by a different file.
    fn invalidate_other_claims(&mut self, device: &str, path: &str, file_id: &str) {
        let catalog = self.catalog;
        if let Some(old) = catalog.copy_at(device, path) {
            if old.file_id != file_id {
                self.invalidate(old);
            }
        }
    }

    fn invalidate_source_overrides(&mut self, device: &str, path: &str, file_id: &str) {
        for override_ in self.catalog.safe_copy_overrides_at(device, path) {
            if override_.source_device_id == device
                && override_.source_path == path
                && override_.file_id != file_id
            {
                self.removed_overrides.insert(override_.id.clone());
            }
        }
        self.overrides.retain(|override_| {
            override_.source_device_id != device
                || override_.source_path != path
                || override_.file_id == file_id
        });
    }

    fn invalidate_destination_overrides(&mut self, device: &str, path: &str) {
        for override_ in self.catalog.safe_copy_overrides_at(device, path) {
            if override_.destination_device_id == device && override_.destination_path == path {
                self.removed_overrides.insert(override_.id.clone());
            }
        }
        self.overrides.retain(|override_| {
            override_.destination_device_id != device || override_.destination_path != path
        });
    }

    pub(super) fn invalidate_override(&mut self, override_: &SafeCopyOverride) {
        self.removed_overrides.insert(override_.id.clone());
    }

    pub(super) fn invalidate(&mut self, copy: &FileCopy) {
        self.invalidate_source_overrides(&copy.device_id, &copy.path, &copy.file_id);
        self.invalidate_destination_overrides(&copy.device_id, &copy.path);
        if self.removed.insert(copy.id.clone()) {
            self.copies.push(FileCopy {
                removed: true,
                ..copy.clone()
            });
        }
    }

    pub(super) fn flush(&mut self) -> crate::store::StoreResult<()> {
        for id in std::mem::take(&mut self.removed_overrides) {
            self.store
                .delete(crate::domain::EntityKind::SafeCopyOverride, &id)?;
        }
        if !self.records.is_empty() {
            self.store.put_all(&std::mem::take(&mut self.records))?;
        }
        if !self.copies.is_empty() {
            self.store.put_all(&std::mem::take(&mut self.copies))?;
        }
        if !self.overrides.is_empty() {
            self.store.put_all(&std::mem::take(&mut self.overrides))?;
        }
        Ok(())
    }
}
