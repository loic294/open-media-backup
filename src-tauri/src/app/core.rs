use super::{entities, files, AppSettings, FilePage, ListFilesRequest, Snapshot};
use crate::plan::{project_status, Catalog, FailureMap, ProjectStatus, RootResolver};
use crate::store::Store;
use crate::thumbnails::ThumbnailCache;
use crate::transfer::{TransferJob, TransferManager};
use parking_lot::Mutex;
use serde_json::Value;
use std::collections::HashSet;
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
        settings.save(&self.store).map_err(|e| e.to_string())
    }

    pub fn project_status(&self, project_id: &str) -> Result<ProjectStatus, String> {
        let catalog = Catalog::load(&self.store).map_err(|e| e.to_string())?;
        let mut status = project_status(
            &self.store,
            self.resolver.as_ref(),
            &catalog,
            project_id,
            &self.failures.lock(),
        )
        .map_err(|e| e.to_string())?;
        for dest in &mut status.destinations {
            dest.free_bytes = dest
                .root_path
                .as_deref()
                .and_then(|p| free_space(Path::new(p)));
        }
        Ok(status)
    }

    pub fn list_files(&self, req: &ListFilesRequest) -> Result<FilePage, String> {
        let page = files::list_files(
            &self.store,
            self.resolver.as_ref(),
            &self.failures.lock(),
            req,
        )?;
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
        Ok(page)
    }

    pub fn thumbnail(&self, path: &Path) -> Result<Option<PathBuf>, String> {
        if crate::media::media_kind(path) == crate::media::MediaKind::Other {
            return Ok(None);
        }
        self.thumbnails
            .get_or_create(path)
            .map_err(|e| e.to_string())
    }

    pub fn media_preview(&self, path: &Path) -> Result<Option<PathBuf>, String> {
        if crate::media::media_kind(path) == crate::media::MediaKind::Other {
            return Ok(None);
        }
        self.thumbnails
            .get_or_create_preview(path)
            .map_err(|e| e.to_string())
    }

    pub fn authorize_video_preview(&self, path: &Path) -> Result<PathBuf, String> {
        let path = self.authorize_media_open(path)?;
        if crate::media::media_kind(&path) != crate::media::MediaKind::Video {
            return Err("The requested file is not a supported video".into());
        }
        Ok(path)
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
