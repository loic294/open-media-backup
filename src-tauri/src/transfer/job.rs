use super::copy::{copy_verified, CopyError};
use super::handle::{JobHandle, JobState};
use crate::domain::{FileCopy, FileRecord};
use crate::paths::{ensure_backup_folder, to_relative};
use crate::plan::{
    classify_flow, resolve_flow, Catalog, Category, FailureMap, FlowContext, PlannedFile,
    RootResolver,
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
    let ctx = prepare(store, resolver, project_id, flow_id)?;
    let catalog = Catalog::load(store).map_err(|e| e.to_string())?;
    let pending: Vec<PlannedFile> = classify_flow(&ctx, &catalog, None)
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
    flow_failures.retain(|rel, _| pending.iter().any(|f| &f.rel_path == rel));
    for file in &pending {
        if handle.checkpoint().is_err() {
            break;
        }
        handle.update(|j| j.current_file = Some(file.rel_path.clone()));
        match transfer_one(&ctx, &catalog, file, handle) {
            Ok((hash, dest_rel)) => {
                flow_failures.remove(&file.rel_path);
                writer.add(&ctx, file, hash, dest_rel);
            }
            Err(CopyError::Cancelled) => break,
            Err(e) => {
                let message = e.to_string();
                handle.update(|j| j.errors.push(format!("{}: {message}", file.rel_path)));
                flow_failures.insert(file.rel_path.clone(), message);
            }
        }
        handle.update(|j| j.files_done += 1);
        if writer.len() >= BATCH {
            writer.flush().map_err(|e| e.to_string())?;
        }
    }
    writer.flush().map_err(|e| e.to_string())?;
    if !flow_failures.is_empty() {
        failures.lock().insert(flow_id.to_string(), flow_failures);
    }
    handle.update(|j| j.current_file = None);
    Ok(())
}

fn prepare(
    store: &Store,
    resolver: &dyn RootResolver,
    project_id: &str,
    flow_id: &str,
) -> Result<FlowContext, String> {
    let mut ctx = resolve_flow(store, resolver, project_id, flow_id).map_err(|e| e.to_string())?;
    if ctx.source_root.is_none() {
        return Err(format!("{} is not connected", ctx.source_device.name));
    }
    if ctx.dest_root.is_none() {
        return Err(format!("{} is not connected", ctx.dest_device.name));
    }
    if let Some(err) = &ctx.config_error {
        return Err(err.clone());
    }
    if ctx.destination.use_backup_marker {
        let root = ctx.source_root.clone().expect("checked above");
        let folder = ctx.vars.get("backup_folder").cloned().unwrap_or_default();
        ensure_backup_folder(&root, &folder).map_err(|e| format!("backup marker: {e}"))?;
        ctx = resolve_flow(store, resolver, project_id, flow_id).map_err(|e| e.to_string())?;
    }
    Ok(ctx)
}

fn transfer_one(
    ctx: &FlowContext,
    catalog: &Catalog,
    file: &PlannedFile,
    handle: &JobHandle,
) -> Result<(String, String), CopyError> {
    let src = file.abs_path.as_ref().ok_or(CopyError::SourceChanged)?;
    let dst = ctx
        .target_abs(&file.rel_path)
        .ok_or(CopyError::SourceChanged)?;
    let known = file
        .file_id
        .as_deref()
        .and_then(|id| catalog.record(id))
        .filter(|r| r.hash_algo == ctx.space.hash_algo)
        .map(|r| r.hash.clone());
    let before = handle.snapshot().bytes_done;
    let outcome = copy_verified(
        src,
        &dst,
        ctx.space.hash_algo,
        ctx.space.verify_mode,
        known.as_deref(),
        handle,
    );
    // Keep the byte counter aligned with the file size, whatever was re-read or skipped.
    handle.update(|j| j.bytes_done = before + file.size);
    let outcome = outcome?;
    let root = ctx.dest_root.as_ref().expect("checked in prepare");
    let dest_rel =
        to_relative(root, &outcome.final_path).unwrap_or_else(|| ctx.target_rel(&file.rel_path));
    Ok((outcome.hash, dest_rel))
}

struct RecordWriter<'a> {
    store: &'a Store,
    catalog: &'a Catalog,
    written: HashSet<String>,
    records: Vec<FileRecord>,
    copies: Vec<FileCopy>,
}

impl<'a> RecordWriter<'a> {
    fn new(store: &'a Store, catalog: &'a Catalog) -> Self {
        Self {
            store,
            catalog,
            written: HashSet::new(),
            records: vec![],
            copies: vec![],
        }
    }

    fn len(&self) -> usize {
        self.copies.len()
    }

    fn add(&mut self, ctx: &FlowContext, file: &PlannedFile, hash: String, dest_rel: String) {
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
        for (device, path) in [
            (&ctx.source_device.id, source_path),
            (&ctx.dest_device.id, dest_rel),
        ] {
            self.copies.push(FileCopy {
                id: FileCopy::id_for(&file_id, device, &path),
                file_id: file_id.clone(),
                device_id: device.clone(),
                path,
                verified_at: now,
                removed: false,
            });
        }
    }

    fn flush(&mut self) -> crate::store::StoreResult<()> {
        if !self.records.is_empty() {
            self.store.put_all(&std::mem::take(&mut self.records))?;
        }
        if !self.copies.is_empty() {
            self.store.put_all(&std::mem::take(&mut self.copies))?;
        }
        Ok(())
    }
}
