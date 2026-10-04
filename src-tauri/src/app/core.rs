use super::{
    entities, files, resolve_reveal_path, reveal_space_id, AppSettings, FilePage, ListFilesRequest,
    ListWorkspaceFilesRequest, RevealKind, Snapshot,
};
use crate::domain::{Destination, DestinationKind};
use crate::plan::{
    workspace_status, Catalog, FailureMap, ProjectStatus, RootResolver, WorkspaceContext,
    WorkspaceStatus,
};
use crate::store::Store;
use crate::thumbnails::ThumbnailCache;
use crate::transfer::{TransferJob, TransferManager};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub type Resolver = Arc<dyn RootResolver + Send + Sync>;

/// Everything the commands operate on.
pub struct AppCore {
    pub store: Arc<Store>,
    pub resolver: Resolver,
    pub transfers: TransferManager,
    pub failures: Arc<Mutex<FailureMap>>,
    pub thumbnails: ThumbnailCache,
    preview_paths: Mutex<HashSet<PathBuf>>,
    app_imports: Mutex<HashMap<String, AppImportSession>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AppImportFile {
    pub rel_path: String,
    pub project_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PreparedAppImport {
    pub token: String,
    pub app_name: String,
    pub app_path: String,
    pub paths: Vec<PathBuf>,
    pub files: Vec<AppImportFile>,
}

#[derive(Debug, Clone)]
struct AppImportSession {
    context: WorkspaceContext,
    flow_id: String,
    files: Vec<AppImportFile>,
}

impl AppCore {
    pub fn new(
        store: Arc<Store>,
        resolver: Resolver,
        thumbnail_dir: PathBuf,
        on_transfers: impl Fn(Vec<TransferJob>) + Send + Sync + 'static,
    ) -> Self {
        Self {
            store,
            resolver,
            transfers: TransferManager::new(on_transfers),
            failures: Arc::default(),
            thumbnails: ThumbnailCache::new(thumbnail_dir),
            preview_paths: Mutex::default(),
            app_imports: Mutex::default(),
        }
    }

    /// Records this computer in the synced catalog so peers can show its name.
    pub fn register_computer(&self) -> Result<(), String> {
        let me = super::snapshot::local_computer(&self.store);
        let known: Option<crate::domain::Computer> =
            self.store.get(&me.id).map_err(|e| e.to_string())?;
        if known
            .as_ref()
            .is_some_and(|k| !k.name.is_empty() && k.os == me.os)
        {
            return Ok(());
        }
        self.store.put(&me).map_err(|e| e.to_string())
    }

    pub fn snapshot(&self) -> Result<Snapshot, String> {
        Snapshot::load(&self.store).map_err(|e| e.to_string())
    }

    pub fn save_entity(&self, kind: &str, entity: Value) -> Result<(), String> {
        entities::save_entity(&self.store, kind, entity)
    }

    pub fn delete_entity(&self, kind: &str, id: &str) -> Result<(), String> {
        entities::delete_entity(&self.store, kind, id)
    }

    pub fn settings(&self) -> AppSettings {
        AppSettings::load(&self.store)
    }

    pub fn save_settings(&self, settings: &AppSettings) -> Result<(), String> {
        let current = AppSettings::load(&self.store);
        let mut settings = settings.clone();
        super::app_paths::normalize_app_settings(&mut settings, &current)?;
        settings.save(&self.store).map_err(|e| e.to_string())
    }

    pub fn project_status(&self, project_id: &str) -> Result<ProjectStatus, String> {
        let context =
            WorkspaceContext::for_project(&self.store, project_id).map_err(|e| e.to_string())?;
        let status = self.workspace_status(&context)?;
        Ok(ProjectStatus {
            project_id: project_id.into(),
            sources: status.sources,
            destinations: status.destinations,
            flows: status.flows,
        })
    }

    pub fn workspace_status(&self, context: &WorkspaceContext) -> Result<WorkspaceStatus, String> {
        let catalog = Catalog::load(&self.store).map_err(|e| e.to_string())?;
        let failures = self.failures.lock().clone();
        let mut status = workspace_status(
            &self.store,
            self.resolver.as_ref(),
            &catalog,
            context,
            &failures,
        )
        .map_err(|e| e.to_string())?;
        let app_destinations: HashSet<String> = self
            .store
            .list::<Destination>()
            .map_err(|e| e.to_string())?
            .into_iter()
            .filter(|d| d.kind == DestinationKind::App)
            .map(|d| d.id)
            .collect();
        for dest in &mut status.destinations {
            dest.free_bytes = (!app_destinations.contains(&dest.destination_id))
                .then(|| {
                    dest.root_path
                        .as_deref()
                        .and_then(|p| free_space(Path::new(p)))
                })
                .flatten();
        }
        Ok(status)
    }

    pub fn list_files(&self, req: &ListFilesRequest) -> Result<FilePage, String> {
        let failures = self.failures.lock().clone();
        let page = files::list_files(&self.store, self.resolver.as_ref(), &failures, req)?;
        self.authorize_preview_paths(&page);
        Ok(page)
    }

    pub fn list_workspace_files(
        &self,
        req: &ListWorkspaceFilesRequest,
    ) -> Result<FilePage, String> {
        let failures = self.failures.lock().clone();
        let page =
            files::list_workspace_files(&self.store, self.resolver.as_ref(), &failures, req)?;
        self.authorize_preview_paths(&page);
        Ok(page)
    }

    fn authorize_preview_paths(&self, page: &FilePage) {
        let mut preview_paths = self.preview_paths.lock();
        for path in page
            .items
            .iter()
            .filter_map(|file| file.abs_path.as_deref())
        {
            if let Ok(path) = Path::new(path).canonicalize() {
                preview_paths.insert(path);
            }
        }
    }

    pub fn thumbnail(&self, path: &Path) -> Result<Option<PathBuf>, String> {
        if crate::media::media_kind(path) == crate::media::MediaKind::Other {
            return Ok(None);
        }
        self.thumbnails
            .get_or_create(path)
            .map_err(|e| e.to_string())
    }

    pub fn authorize_media_open(&self, path: &Path) -> Result<PathBuf, String> {
        let path = path
            .canonicalize()
            .map_err(|e| format!("Could not resolve media path: {e}"))?;
        if !self.preview_paths.lock().contains(&path) {
            return Err("Opening media is only available for files listed by a source".into());
        }
        if !path.is_file() {
            return Err("The requested media path is not a file".into());
        }
        Ok(path)
    }

    pub fn reveal_file_manager_path(
        &self,
        project_id: Option<&str>,
        kind: RevealKind,
        id: &str,
    ) -> Result<PathBuf, String> {
        let project_id = project_id.map(str::to_string).or_else(|| {
            reveal_space_id(&self.store, kind, id)
                .ok()
                .and_then(|space_id| {
                    self.settings()
                        .active_project_by_space
                        .get(&space_id)
                        .cloned()
                })
        });
        resolve_reveal_path(
            &self.store,
            self.resolver.as_ref(),
            project_id.as_deref(),
            kind,
            id,
        )
        .map(|path| nearest_existing(&path))
    }

    pub fn prepare_app_import(
        &self,
        project_id: &str,
        flow_id: &str,
    ) -> Result<PreparedAppImport, String> {
        let context =
            WorkspaceContext::for_project(&self.store, project_id).map_err(|e| e.to_string())?;
        self.prepare_workspace_app_import(&context, flow_id)
    }

    pub fn prepare_workspace_app_import(
        &self,
        context: &WorkspaceContext,
        flow_id: &str,
    ) -> Result<PreparedAppImport, String> {
        use crate::domain::DestinationKind;
        use crate::plan::{classify_flow, resolve_workspace_flow, Category};

        let ctx = resolve_workspace_flow(&self.store, self.resolver.as_ref(), context, flow_id)
            .map_err(|e| e.to_string())?;
        if ctx.destination.kind != DestinationKind::App {
            return Err("This destination is not an app destination".into());
        }
        if let Some(error) = &ctx.config_error {
            return Err(error.clone());
        }
        if ctx.source_root.is_none() {
            return Err(format!("{} is not connected", ctx.source_device.name));
        }
        let app_name = ctx.dest_device.name.clone();
        let app_path = self
            .settings()
            .app_destinations
            .get(&ctx.destination.id)
            .map(String::as_str)
            .map(str::trim)
            .filter(|path| !path.is_empty())
            .map(Path::new)
            .map(super::app_paths::validate_app_path)
            .transpose()?
            .map(|path| super::app_paths::path_to_string(&path))
            .ok_or_else(|| format!("Choose the application for {app_name} on this computer"))?;
        let catalog = Catalog::load(&self.store).map_err(|e| e.to_string())?;
        let failures = self.failures.lock().get(flow_id).cloned();
        let planned: Vec<_> = classify_flow(&ctx, &catalog, failures.as_ref())
            .into_iter()
            .filter(|f| f.category == Category::ToTransfer)
            .collect();
        if planned.is_empty() {
            return Err("No files to import".into());
        }
        let mut paths = Vec::new();
        let mut seen_paths = HashSet::new();
        let mut files = Vec::new();
        for file in planned {
            let path = file
                .abs_path
                .clone()
                .ok_or_else(|| format!("{} is not available on disk", file.rel_path))?;
            if seen_paths.insert(path.clone()) {
                paths.push(path);
            }
            files.push(AppImportFile {
                rel_path: file.rel_path,
                project_id: file.project_id,
            });
        }
        let token = crate::domain::new_id();
        self.app_imports.lock().insert(
            token.clone(),
            AppImportSession {
                context: context.clone(),
                flow_id: flow_id.to_string(),
                files: files.clone(),
            },
        );
        Ok(PreparedAppImport {
            token,
            app_name,
            app_path,
            paths,
            files,
        })
    }

    pub fn confirm_app_import(
        &self,
        project_id: &str,
        flow_id: &str,
        token: &str,
    ) -> Result<usize, String> {
        let context =
            WorkspaceContext::for_project(&self.store, project_id).map_err(|e| e.to_string())?;
        self.confirm_workspace_app_import(&context, flow_id, token)
    }

    pub fn confirm_workspace_app_import(
        &self,
        context: &WorkspaceContext,
        flow_id: &str,
        token: &str,
    ) -> Result<usize, String> {
        use crate::domain::{DestinationKind, FileCopy, FileRecord};
        use crate::plan::{classify_flow, resolve_workspace_flow, Category};

        let session = self
            .app_imports
            .lock()
            .remove(token)
            .ok_or_else(|| "This app import confirmation is no longer valid".to_string())?;
        if session.context != *context || session.flow_id != flow_id {
            return Err("This app import confirmation does not match the flow".into());
        }
        let ctx = resolve_workspace_flow(&self.store, self.resolver.as_ref(), context, flow_id)
            .map_err(|e| e.to_string())?;
        if ctx.destination.kind != DestinationKind::App {
            return Err("This destination is not an app destination".into());
        }
        let catalog = Catalog::load(&self.store).map_err(|e| e.to_string())?;
        let failures = self.failures.lock().get(flow_id).cloned();
        let allowed: HashSet<AppImportFile> = classify_flow(&ctx, &catalog, failures.as_ref())
            .into_iter()
            .filter(|f| f.category == Category::ToTransfer)
            .map(|f| AppImportFile {
                rel_path: f.rel_path,
                project_id: f.project_id,
            })
            .collect();
        let requested: HashSet<AppImportFile> = session.files.into_iter().collect();
        let mut records = Vec::new();
        let mut copies = Vec::new();
        let now = chrono::Utc::now().timestamp_millis();
        let mut marked = 0;
        for file in requested.intersection(&allowed) {
            let abs = ctx
                .source_folder()
                .ok_or_else(|| format!("{} is not connected", ctx.source_device.name))?
                .join(&file.rel_path);
            if !abs.is_file() {
                continue;
            }
            let hash = crate::hashing::hash_file(&abs, ctx.space.hash_algo, |_| true)
                .map_err(|e| format!("{}: {e}", file.rel_path))?;
            let file_id = FileRecord::id_for(ctx.space.hash_algo, &hash);
            if catalog.record(&file_id).is_none() {
                records.push(FileRecord {
                    id: file_id.clone(),
                    hash,
                    hash_algo: ctx.space.hash_algo,
                    size: abs.metadata().map_err(|e| e.to_string())?.len(),
                    name: file
                        .rel_path
                        .rsplit('/')
                        .next()
                        .unwrap_or_default()
                        .to_string(),
                    origin_device_id: ctx.source_device.id.clone(),
                    origin_path: ctx.source_device_path(&file.rel_path),
                    modified_at: None,
                });
            }
            let source_path = ctx.source_device_path(&file.rel_path);
            let app_path = match &file.project_id {
                Some(project_id) => format!("{project_id}/{}", file.rel_path),
                None => file.rel_path.clone(),
            };
            for (device, path) in [
                (&ctx.source_device.id, source_path),
                (&ctx.destination.id, app_path),
            ] {
                copies.push(FileCopy {
                    id: FileCopy::id_for(&file_id, device, &path),
                    file_id: file_id.clone(),
                    device_id: device.clone(),
                    path,
                    verified_at: now,
                    removed: false,
                });
            }
            marked += 1;
        }
        if !records.is_empty() {
            self.store.put_all(&records).map_err(|e| e.to_string())?;
        }
        if !copies.is_empty() {
            self.store.put_all(&copies).map_err(|e| e.to_string())?;
        }
        Ok(marked)
    }
}

fn free_space(path: &Path) -> Option<u64> {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    disks
        .list()
        .iter()
        .filter(|d| path.starts_with(d.mount_point()))
        .max_by_key(|d| d.mount_point().as_os_str().len())
        .map(|d| d.available_space())
}

/// Project folders are created on first transfer; reveal the closest folder that exists.
fn nearest_existing(path: &std::path::Path) -> PathBuf {
    path.ancestors()
        .find(|p| p.exists())
        .unwrap_or(path)
        .to_path_buf()
}
