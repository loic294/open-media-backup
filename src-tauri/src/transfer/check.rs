use super::copy::{compare_files_with_progress, hash_checked_with_progress, Comparison, CopyError};
use super::handle::{CheckItem, CheckOutcome, CheckResults, JobHandle, JobState};
use super::job::{prepare, RecordWriter};
use crate::domain::{DestinationKind, FileCopy};
use crate::paths::join_relative;
use crate::plan::{
    classify_flow, Catalog, Category, FlowContext, PlannedFile, RootResolver, WorkspaceContext,
};
use crate::store::Store;

const BATCH: usize = 50;

/// Hashes every eligible source file against its expected destination path. Media files
/// and backup markers are never written; verified matches are recorded in the catalog and
/// location claims that are demonstrably wrong are invalidated. Check throughput is not
/// fed into the learned transfer speeds.
pub fn run_workspace_check(
    store: &Store,
    resolver: &dyn RootResolver,
    context: &WorkspaceContext,
    flow_id: &str,
    handle: &JobHandle,
) -> Result<(), String> {
    let ctx = prepare(store, resolver, context, flow_id, false)?;
    if ctx.destination.kind != DestinationKind::Folder {
        return Err("Only folder destinations can be checked".into());
    }
    let catalog = Catalog::load(store).map_err(|e| e.to_string())?;
    let planned = classify_flow(&ctx, &catalog, None);
    let (eligible, broken): (Vec<_>, Vec<_>) = planned
        .iter()
        .filter(|f| f.category != Category::Ignored)
        .partition(|f| f.category != Category::Error);
    handle.update(|j| {
        j.label = ctx.label();
        j.files_total = eligible.len();
        j.bytes_total = eligible.iter().map(|f| f.size).sum();
        if j.state != JobState::Paused {
            j.state = JobState::Running;
        }
    });

    let mut results = CheckResults::default();
    for file in broken {
        results.add(CheckItem {
            source_path: file.rel_path.clone(),
            destination_path: file.target_path.clone().unwrap_or_default(),
            outcome: CheckOutcome::Error,
            error: Some(
                file.error
                    .clone()
                    .unwrap_or_else(|| "cannot be checked".into()),
            ),
        });
    }
    let mut writer = RecordWriter::new(store, &catalog);
    for file in eligible {
        if handle.checkpoint().is_err() {
            break;
        }
        let before = handle.snapshot().bytes_done;
        handle.update(|j| j.current_file = Some(file.rel_path.clone()));
        match check_one(&ctx, &catalog, file, handle, &mut writer) {
            Ok(item) => results.add(item),
            Err(CopyError::Cancelled) => break,
            Err(error) => results.add(CheckItem {
                source_path: file.rel_path.clone(),
                destination_path: file.target_path.clone().unwrap_or_default(),
                outcome: CheckOutcome::Error,
                error: Some(error.to_string()),
            }),
        }
        handle.update(|j| {
            j.files_done += 1;
            j.bytes_done = before + file.size;
        });
        if writer.len() >= BATCH {
            writer.flush().map_err(|e| e.to_string())?;
        }
    }
    // Only completed, verified results are persisted, even after a cancellation.
    writer.flush().map_err(|e| e.to_string())?;
    handle.update(|j| {
        j.current_file = None;
        j.check_results = Some(results);
    });
    Ok(())
}

fn check_one(
    ctx: &FlowContext,
    catalog: &Catalog,
    file: &PlannedFile,
    handle: &JobHandle,
    writer: &mut RecordWriter<'_>,
) -> Result<CheckItem, CopyError> {
    let src = file.abs_path.as_ref().ok_or(CopyError::SourceChanged)?;
    let root = ctx.dest_root.as_ref().ok_or(CopyError::SourceChanged)?;
    let target = file.target_path.clone().ok_or(CopyError::SourceChanged)?;
    let dst = join_relative(root, &target);
    let algo = ctx.space.hash_algo;
    let item = |outcome| CheckItem {
        source_path: file.rel_path.clone(),
        destination_path: target.clone(),
        outcome,
        error: None,
    };
    let claim = catalog.copy_at(&ctx.dest_device.id, &target);
    let source_claim = catalog.copy_at(
        &ctx.source_device.id,
        &ctx.source_device_path(&file.rel_path),
    );
    let mut fresh_source = None;
    let before = handle.snapshot().bytes_done;
    let source_budget = file.size / 2;
    let mut source_progress = HashProgress::new(handle, before, source_budget);
    let mut destination_progress =
        HashProgress::new(handle, before + source_budget, file.size - source_budget);
    match compare_files_with_progress(
        src,
        &dst,
        algo,
        handle,
        &mut fresh_source,
        |source, bytes, total| {
            if source {
                source_progress.add(bytes, total);
            } else {
                destination_progress.add(bytes, total);
            }
        },
    )? {
        Comparison::Missing => {
            if let Some(claim) = claim {
                writer.invalidate(claim);
            }
            if let Some(claim) = source_claim {
                let mut progress = HashProgress::new(handle, before, file.size);
                let hash = hash_checked_with_progress(src, algo, handle, |bytes, total| {
                    progress.add(bytes, total);
                })?;
                if differs(catalog, claim, algo, &hash) {
                    writer.invalidate(claim);
                }
            }
            Ok(item(CheckOutcome::Missing))
        }
        Comparison::Match(hash) => {
            writer.add(ctx, file, hash, target.clone());
            Ok(item(CheckOutcome::Matched))
        }
        Comparison::Different {
            source_hash,
            destination_hash,
        } => {
            if let Some(claim) = claim {
                if differs(catalog, claim, algo, &destination_hash) {
                    writer.invalidate(claim);
                }
            }
            if let Some(claim) = source_claim {
                if differs(catalog, claim, algo, &source_hash) {
                    writer.invalidate(claim);
                }
            }
            Ok(item(CheckOutcome::Conflict))
        }
    }
}

struct HashProgress<'a> {
    handle: &'a JobHandle,
    before: u64,
    budget: u64,
    read: u64,
}

impl<'a> HashProgress<'a> {
    fn new(handle: &'a JobHandle, before: u64, budget: u64) -> Self {
        Self {
            handle,
            before,
            budget,
            read: 0,
        }
    }

    fn add(&mut self, bytes: u64, total: u64) {
        self.read = self.read.saturating_add(bytes);
        let done = if total == 0 {
            self.budget
        } else {
            (u128::from(self.read.min(total)) * u128::from(self.budget) / u128::from(total)) as u64
        };
        self.handle
            .update(|j| j.bytes_done = j.bytes_done.max(self.before + done));
    }
}

/// Only a claim whose recorded hash is comparable and different is demonstrably stale.
fn differs(
    catalog: &Catalog,
    claim: &FileCopy,
    algo: crate::domain::HashAlgo,
    actual: &str,
) -> bool {
    catalog
        .record(&claim.file_id)
        .is_some_and(|r| r.hash_algo == algo && r.hash != actual)
}

impl CheckResults {
    fn add(&mut self, item: CheckItem) {
        match item.outcome {
            CheckOutcome::Matched => {
                self.matched += 1;
                return;
            }
            CheckOutcome::Missing => self.missing += 1,
            CheckOutcome::Conflict => self.conflicts += 1,
            CheckOutcome::Error => self.errors += 1,
        }
        self.items.push(item);
    }
}
