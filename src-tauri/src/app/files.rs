use crate::media::{media_kind, MediaKind};
use crate::plan::{classify_flow, resolve_flow, Catalog, Category, FailureMap, RootResolver};
use crate::store::Store;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListFilesRequest {
    pub project_id: String,
    pub flow_id: String,
    pub category: Category,
    pub offset: usize,
    pub limit: usize,
    pub filter: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileEntry {
    pub rel_path: String,
    pub name: String,
    pub size: u64,
    pub media: MediaKind,
    pub abs_path: Option<String>,
    pub target_path: Option<String>,
    pub category: Category,
    pub error: Option<String>,
    pub capture_time: Option<i64>,
    pub project_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FilePage {
    pub total: usize,
    pub total_bytes: u64,
    pub items: Vec<FileEntry>,
}

pub fn list_files(
    store: &Store,
    resolver: &dyn RootResolver,
    failures: &FailureMap,
    req: &ListFilesRequest,
) -> Result<FilePage, String> {
    let ctx =
        resolve_flow(store, resolver, &req.project_id, &req.flow_id).map_err(|e| e.to_string())?;
    let catalog = Catalog::load(store).map_err(|e| e.to_string())?;
    let filter = req
        .filter
        .as_deref()
        .map(str::to_lowercase)
        .filter(|f| !f.is_empty());
    let matching: Vec<_> = classify_flow(&ctx, &catalog, failures.get(&req.flow_id))
        .into_iter()
        .filter(|f| f.category == req.category)
        .filter(|f| {
            filter
                .as_ref()
                .is_none_or(|q| f.rel_path.to_lowercase().contains(q))
        })
        .collect();
    Ok(FilePage {
        total: matching.len(),
        total_bytes: matching.iter().map(|f| f.size).sum(),
        items: matching
            .into_iter()
            .skip(req.offset)
            .take(req.limit.clamp(1, 1000))
            .map(|f| FileEntry {
                name: f
                    .rel_path
                    .rsplit('/')
                    .next()
                    .unwrap_or_default()
                    .to_string(),
                media: media_kind(Path::new(&f.rel_path)),
                abs_path: f.abs_path.as_ref().map(|p| p.display().to_string()),
                target_path: f.target_path,
                rel_path: f.rel_path,
                size: f.size,
                category: f.category,
                error: f.error,
                capture_time: f.capture_time,
                project_id: f.project_id,
            })
            .collect(),
    })
}
