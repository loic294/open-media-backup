mod embedded_jpeg;
mod image_thumb;
mod jpeg_scaled;
mod key;
mod orientation;
mod video;

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::media::{media_kind, MediaKind};

#[derive(Debug, Clone)]
pub struct ThumbnailCache {
    dir: PathBuf,
}

impl ThumbnailCache {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    pub fn get_or_create(&self, path: &Path) -> Result<Option<PathBuf>, ThumbnailError> {
        self.get_or_create_at_size(path, false)
    }

    pub fn get_or_create_preview(&self, path: &Path) -> Result<Option<PathBuf>, ThumbnailError> {
        self.get_or_create_at_size(path, true)
    }

    fn get_or_create_at_size(
        &self,
        path: &Path,
        preview: bool,
    ) -> Result<Option<PathBuf>, ThumbnailError> {
        let metadata = fs::metadata(path).map_err(|source| ThumbnailError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let key = key::cache_key(path, &metadata)?;
        let cached_name = if preview {
            format!("{key}-preview.jpg")
        } else {
            format!("{key}.jpg")
        };
        let cached = self.dir.join(cached_name);
        if cached.exists() {
            return Ok(Some(cached));
        }

        let kind = media_kind(path);
        if matches!(kind, MediaKind::Other) {
            return Ok(None);
        }

        fs::create_dir_all(&self.dir).map_err(|source| ThumbnailError::Io {
            path: self.dir.clone(),
            source,
        })?;
        if cached.exists() {
            return Ok(Some(cached));
        }

        let temp = self.temp_path(&key, preview);
        let created = match (kind, preview) {
            (MediaKind::Image | MediaKind::Raw, true) => {
                image_thumb::create_image_preview(path, &temp)?
            }
            (MediaKind::Image | MediaKind::Raw, false) => {
                image_thumb::create_image_thumbnail(path, &temp)?
            }
            (MediaKind::Video, true) => video::create_video_preview(path, &temp)?,
            (MediaKind::Video, false) => video::create_video_thumbnail(path, &temp)?,
            (MediaKind::Other, _) => false,
        };

        if !created {
            let _ = fs::remove_file(&temp);
            return Ok(None);
        }

        match fs::rename(&temp, &cached) {
            Ok(()) => Ok(Some(cached)),
            Err(err) if cached.exists() => {
                let _ = fs::remove_file(&temp);
                log::debug!("thumbnail cache race for {}: {err}", cached.display());
                Ok(Some(cached))
            }
            Err(source) => Err(ThumbnailError::Io {
                path: cached,
                source,
            }),
        }
    }

    fn temp_path(&self, key: &str, preview: bool) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let thread = format!("{:?}", std::thread::current().id());
        self.dir.join(format!(
            ".{key}{}.{}.{}.{}.tmp",
            if preview { "-preview" } else { "" },
            std::process::id(),
            thread.replace(['(', ')', ' '], ""),
            nanos
        ))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ThumbnailError {
    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("image error: {0}")]
    Image(#[from] image::ImageError),
    #[error("image reader error: {0}")]
    ImageReader(std::io::Error),
    #[error("failed to start ffmpeg: {source}")]
    FfmpegSpawn {
        #[source]
        source: std::io::Error,
    },
    #[error("ffmpeg timed out while creating thumbnail")]
    FfmpegTimeout,
}

#[cfg(test)]
mod tests;
