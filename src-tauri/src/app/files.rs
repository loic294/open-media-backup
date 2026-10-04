use crate::media::{media_kind, MediaKind};
use crate::plan::{
    classify_flow, resolve_workspace_flow, Catalog, Category, FailureMap, RootResolver,
    WorkspaceContext,
};
use crate::store::Store;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DirectoryKind {
    Destination,
    Source,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FileDirectory {
    pub kind: DirectoryKind,
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DirectorySummary {
    pub kind: DirectoryKind,
    pub path: String,
    pub total: usize,
    pub total_bytes: u64,
    pub direct_files: usize,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListFilesRequest {
    pub project_id: String,
    pub flow_id: String,
    pub category: Category,
    pub offset: usize,
    pub limit: usize,
    pub filter: Option<String>,
    pub directory: Option<FileDirectory>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListWorkspaceFilesRequest {
    pub context: WorkspaceContext,
    pub flow_id: String,
    pub category: Category,
    pub offset: usize,
    pub limit: usize,
    pub filter: Option<String>,
    pub directory: Option<FileDirectory>,
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
    pub directories: Vec<DirectorySummary>,
}

pub fn list_files(
    store: &Store,
    resolver: &dyn RootResolver,
    failures: &FailureMap,
    req: &ListFilesRequest,
) -> Result<FilePage, String> {
    let context =
        WorkspaceContext::for_project(store, &req.project_id).map_err(|e| e.to_string())?;
    list_workspace_files(
        store,
        resolver,
        failures,
        &ListWorkspaceFilesRequest {
            context,
            flow_id: req.flow_id.clone(),
            category: req.category,
            offset: req.offset,
            limit: req.limit,
            filter: req.filter.clone(),
            directory: req.directory.clone(),
        },
    )
}

pub fn list_workspace_files(
    store: &Store,
    resolver: &dyn RootResolver,
    failures: &FailureMap,
    req: &ListWorkspaceFilesRequest,
) -> Result<FilePage, String> {
    let ctx = resolve_workspace_flow(store, resolver, &req.context, &req.flow_id)
        .map_err(|e| e.to_string())?;
    if !ctx.source_path_valid {
        return Err(ctx
            .config_error
            .clone()
            .unwrap_or_else(|| "Invalid source path".into()));
    }
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
            filter.as_ref().is_none_or(|q| {
                f.rel_path.to_lowercase().contains(q)
                    || f.target_path
                        .as_ref()
                        .is_some_and(|p| p.to_lowercase().contains(q))
            })
        })
        .collect();
    let route = |file: &crate::plan::PlannedFile| {
        if ctx.destination.kind == crate::domain::DestinationKind::App {
            (DirectoryKind::Source, file.rel_path.clone())
        } else {
            file.target_path.as_ref().map_or_else(
                || (DirectoryKind::Source, file.rel_path.clone()),
                |path| (DirectoryKind::Destination, path.clone()),
            )
        }
    };
    let mut directories: BTreeMap<(DirectoryKind, String), DirectorySummary> = BTreeMap::new();
    for file in &matching {
        let (kind, path) = route(file);
        let parent = path.rsplit_once('/').map_or("", |(parent, _)| parent);
        let mut current = Some(parent);
        while let Some(path) = current {
            let summary = directories
                .entry((kind, path.to_string()))
                .or_insert_with(|| DirectorySummary {
                    kind,
                    path: path.to_string(),
                    total: 0,
                    total_bytes: 0,
                    direct_files: 0,
                });
            summary.total += 1;
            summary.total_bytes += file.size;
            summary.direct_files += usize::from(path == parent);
            current = if path.is_empty() {
                None
            } else {
                Some(path.rsplit_once('/').map_or("", |(parent, _)| parent))
            };
        }
    }
    let matching: Vec<_> = matching
        .into_iter()
        .filter(|file| {
            req.directory.as_ref().is_none_or(|directory| {
                let (kind, path) = route(file);
                kind == directory.kind
                    && path.rsplit_once('/').map_or("", |(parent, _)| parent) == directory.path
            })
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
        directories: directories.into_values().collect(),
    })
}
