use crate::{
    validate_relative, Browse, HashRequest, HashResponse, Hello, ListedFile, Listing, Root,
};
use axum::{
    extract::{ConnectInfo, DefaultBodyLimit, Path, Query, State},
    http::{header, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use cap_std::{ambient_authority, fs::Dir};
use omb_hash::{constant_time_eq, hash_reader};
use serde::Deserialize;
use std::{
    io,
    net::SocketAddr,
    path::{Path as FsPath, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Instant, UNIX_EPOCH},
};
use tokio::sync::Semaphore;

struct ExposedRoot {
    info: Root,
    canonical: PathBuf,
    dir: Dir,
}

#[derive(Clone)]
pub struct HashServer {
    hello: Hello,
    token: Arc<String>,
    roots: Arc<Vec<ExposedRoot>>,
    permits: Arc<Semaphore>,
}

impl HashServer {
    pub fn new(
        server_id: String,
        name: String,
        token: String,
        paths: Vec<PathBuf>,
        concurrency: usize,
    ) -> io::Result<Self> {
        if token.is_empty() || token.contains(['\r', '\n']) || concurrency == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid server configuration",
            ));
        }
        let mut roots = Vec::new();
        for path in paths {
            let canonical = path.canonicalize()?;
            if !canonical.is_dir() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "root is not a directory",
                ));
            }
            let path = canonical
                .to_str()
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "root is not UTF-8"))?;
            let mut id = omb_hash::hasher(omb_hash::HashAlgo::Blake3);
            id.update(path.as_bytes());
            let info = Root {
                id: id.finish_hex(),
                name: canonical
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into(),
                path: path.into(),
            };
            let dir = Dir::open_ambient_dir(&canonical, ambient_authority())?;
            roots.push(ExposedRoot {
                info,
                canonical,
                dir,
            });
        }
        roots.sort_by(|a, b| a.info.path.cmp(&b.info.path));
        roots.dedup_by(|a, b| a.info.id == b.info.id);
        Ok(Self {
            hello: Hello {
                server_id,
                name,
                version: crate::VERSION.into(),
                kind: "hash_server".into(),
            },
            token: Arc::new(token),
            roots: Arc::new(roots),
            permits: Arc::new(Semaphore::new(concurrency)),
        })
    }

    pub fn roots(&self) -> Vec<Root> {
        self.roots.iter().map(|r| r.info.clone()).collect()
    }

    fn resolve<'a>(
        &'a self,
        selection: &str,
        relative: &str,
    ) -> io::Result<(&'a ExposedRoot, PathBuf)> {
        let (id, subfolder) = selection.split_once('/').unwrap_or((selection, ""));
        validate_relative(subfolder)?;
        validate_relative(relative)?;
        let root = self
            .roots
            .iter()
            .find(|r| r.info.id == id)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "unknown root"))?;
        let joined = FsPath::new(subfolder).join(relative);
        // Also enforce the selected subfolder boundary, not just the exposed root.
        let base = root.canonical.join(subfolder).canonicalize()?;
        let path = root.canonical.join(&joined).canonicalize()?;
        if !base.starts_with(&root.canonical) || !path.starts_with(&base) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "path escapes root",
            ));
        }
        Ok((root, joined))
    }

    fn hash(&self, request: HashRequest, cancel: &AtomicBool) -> io::Result<HashResponse> {
        if cancel.load(Ordering::Relaxed) {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "hashing cancelled",
            ));
        }
        let (root, _) = self.resolve(&request.root, &request.rel_path)?;
        // Capability-scoped open enforces containment again during path resolution,
        // so a symlink swapped after canonicalize cannot expose another filesystem path.
        let subfolder = request.root.split_once('/').map(|(_, p)| p).unwrap_or("");
        let base = root
            .dir
            .open_dir(if subfolder.is_empty() { "." } else { subfolder })?;
        let mut options = cap_std::fs::OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use cap_std::fs::OpenOptionsExt;
            // Opening a FIFO must not occupy a hashing slot indefinitely.
            options.custom_flags(libc::O_NONBLOCK);
        }
        let mut file = base.open_with(&request.rel_path, &options)?;
        let before = file.metadata()?;
        if !before.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "not a regular file",
            ));
        }
        let mut size = 0;
        let hash = hash_reader(&mut file, request.algo, |bytes| {
            size += bytes;
            !cancel.load(Ordering::Relaxed)
        })?;
        if cancel.load(Ordering::Relaxed) {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "hashing cancelled",
            ));
        }
        let after = file.metadata()?;
        if size != before.len() || size != after.len() || before.modified()? != after.modified()? {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "file changed while hashing",
            ));
        }
        Ok(HashResponse {
            hash,
            size,
            modified: after
                .modified()?
                .into_std()
                .duration_since(UNIX_EPOCH)
                .ok()
                .map(|d| d.as_millis() as u64),
        })
    }

    fn browse(&self, root: &str, path: &str) -> io::Result<Browse> {
        let (root, relative) = self.resolve(root, path)?;
        let dir = root.dir.open_dir(if relative.as_os_str().is_empty() {
            FsPath::new(".")
        } else {
            &relative
        })?;
        let mut directories = Vec::new();
        for entry in dir.entries()? {
            let entry = entry?;
            // Do not offer symlinks as mappings. Hash opens still protect against swaps.
            if entry.file_type()?.is_dir() {
                let name = entry.file_name().into_string().map_err(|_| {
                    io::Error::new(io::ErrorKind::InvalidData, "directory is not UTF-8")
                })?;
                directories.push(name);
            }
        }
        directories.sort();
        Ok(Browse {
            path: path.into(),
            directories,
        })
    }

    fn list(&self, root: &str, path: &str) -> io::Result<Listing> {
        let (root, relative) = self.resolve(root, path)?;
        let dir = root.dir.open_dir(if relative.as_os_str().is_empty() {
            FsPath::new(".")
        } else {
            &relative
        })?;
        let mut listing = Listing {
            path: path.into(),
            directories: Vec::new(),
            files: Vec::new(),
            other: Vec::new(),
        };
        for entry in dir.entries()? {
            let entry = entry?;
            let Ok(name) = entry.file_name().into_string() else {
                listing
                    .other
                    .push(entry.file_name().to_string_lossy().into_owned());
                continue;
            };
            // Synology metadata folders are hidden from SMB clients too.
            if name == "@eaDir" {
                continue;
            }
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                listing.directories.push(name);
            } else if file_type.is_file() {
                let metadata = entry.metadata()?;
                listing.files.push(ListedFile {
                    name,
                    size: metadata.len(),
                    modified: metadata
                        .modified()
                        .ok()
                        .and_then(|m| m.into_std().duration_since(UNIX_EPOCH).ok())
                        .map(|d| d.as_millis() as u64),
                });
            } else {
                listing.other.push(name);
            }
        }
        listing.directories.sort();
        listing.files.sort_by(|a, b| a.name.cmp(&b.name));
        listing.other.sort();
        Ok(listing)
    }

    fn root_name(&self, selection: &str) -> String {
        let (id, subfolder) = selection.split_once('/').unwrap_or((selection, ""));
        let name = self
            .roots
            .iter()
            .find(|r| r.info.id == id)
            .map(|r| r.info.name.as_str())
            .unwrap_or("unknown-root");
        if subfolder.is_empty() {
            name.into()
        } else {
            format!("{name}/{subfolder}")
        }
    }
}

pub fn router(server: HashServer) -> Router {
    Router::new()
        .route("/v1/hello", get(hello))
        .route("/v1/roots", get(roots))
        .route("/v1/roots/{id}/browse", get(browse))
        .route("/v1/roots/{id}/list", get(list))
        .route("/v1/hash", post(hash))
        .layer(DefaultBodyLimit::max(16 * 1024))
        .layer(middleware::from_fn_with_state(server.clone(), auth))
        .layer(middleware::from_fn(log_request))
        .with_state(server)
}

/// One line per request, including rejected ones. Tokens and bodies are never logged.
async fn log_request(req: axum::extract::Request, next: Next) -> Response {
    let started = Instant::now();
    let method = req.method().clone();
    let path = req.uri().path().to_owned();
    let peer = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|info| info.0.ip().to_string())
        .unwrap_or_else(|| "-".into());
    let response = next.run(req).await;
    let detail = response
        .extensions()
        .get::<LogDetail>()
        .map(|d| format!(" {}", d.0))
        .unwrap_or_default();
    println!(
        "{} {peer} {method} {path} {} {}ms{detail}",
        timestamp(),
        response.status().as_u16(),
        started.elapsed().as_millis(),
    );
    response
}

#[derive(Clone)]
struct LogDetail(String);

fn with_detail(mut response: Response, detail: String) -> Response {
    response.extensions_mut().insert(LogDetail(detail));
    response
}

/// RFC 3339 UTC timestamp without pulling in a date library.
fn timestamp() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs() as i64;
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Howard Hinnant's civil-from-days algorithm.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        rem / 3_600,
        rem % 3_600 / 60,
        rem % 60,
        now.subsec_millis()
    )
}

async fn auth(
    State(server): State<HashServer>,
    req: axum::extract::Request,
    next: Next,
) -> Response {
    let token = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");
    if !constant_time_eq(token, &server.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    next.run(req).await
}

async fn hello(State(server): State<HashServer>) -> Json<Hello> {
    Json(server.hello)
}
async fn roots(State(server): State<HashServer>) -> Json<Vec<Root>> {
    Json(server.roots())
}

struct CancelOnDrop(Arc<AtomicBool>);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

async fn hash(State(server): State<HashServer>, Json(req): Json<HashRequest>) -> Response {
    let detail = format!(
        "hash {:?} {}/{}",
        req.algo,
        server.root_name(&req.root),
        req.rel_path
    );
    let Ok(permit) = server.permits.clone().try_acquire_owned() else {
        return with_detail(
            (StatusCode::TOO_MANY_REQUESTS, "hash server is busy").into_response(),
            format!("{detail} busy"),
        );
    };
    let flag = Arc::new(AtomicBool::new(false));
    let _guard = CancelOnDrop(flag.clone());
    let response = match tokio::task::spawn_blocking(move || {
        let _permit = permit;
        server.hash(req, &flag)
    })
    .await
    {
        Ok(Ok(result)) => {
            let detail = format!("{detail} {} bytes", result.size);
            return with_detail(Json(result).into_response(), detail);
        }
        Ok(Err(error)) => return with_detail(io_response(&error), format!("{detail}: {error}")),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "hash worker failed").into_response(),
    };
    with_detail(response, detail)
}

#[derive(Deserialize)]
struct BrowseQuery {
    #[serde(default)]
    path: String,
}

async fn browse(
    State(server): State<HashServer>,
    Path(id): Path<String>,
    Query(query): Query<BrowseQuery>,
) -> Response {
    let Ok(permit) = server.permits.clone().try_acquire_owned() else {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    };
    match tokio::task::spawn_blocking(move || {
        let _permit = permit;
        server.browse(&id, &query.path)
    })
    .await
    {
        Ok(Ok(result)) => Json(result).into_response(),
        Ok(Err(error)) => io_response(&error),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

async fn list(
    State(server): State<HashServer>,
    Path(id): Path<String>,
    Query(query): Query<BrowseQuery>,
) -> Response {
    let detail = format!("list {}/{}", server.root_name(&id), query.path);
    let Ok(permit) = server.permits.clone().try_acquire_owned() else {
        return with_detail(
            StatusCode::TOO_MANY_REQUESTS.into_response(),
            format!("{detail} busy"),
        );
    };
    match tokio::task::spawn_blocking(move || {
        let _permit = permit;
        server.list(&id, &query.path)
    })
    .await
    {
        Ok(Ok(result)) => {
            let detail = format!(
                "{detail} {} dirs {} files",
                result.directories.len(),
                result.files.len()
            );
            with_detail(Json(result).into_response(), detail)
        }
        Ok(Err(error)) => with_detail(io_response(&error), format!("{detail}: {error}")),
        Err(_) => with_detail(StatusCode::INTERNAL_SERVER_ERROR.into_response(), detail),
    }
}

fn io_response(error: &io::Error) -> Response {
    let status = match error.kind() {
        io::ErrorKind::NotFound => StatusCode::NOT_FOUND,
        io::ErrorKind::PermissionDenied => StatusCode::FORBIDDEN,
        io::ErrorKind::InvalidInput => StatusCode::BAD_REQUEST,
        io::ErrorKind::InvalidData => StatusCode::CONFLICT,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (
        status,
        status.canonical_reason().unwrap_or("hash server error"),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use omb_hash::HashAlgo;

    #[tokio::test]
    async fn hashing_is_bounded_and_cancel_guard_interrupts_worker() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("file"), b"abc").unwrap();
        let server = HashServer::new(
            "id".into(),
            "test".into(),
            "token".into(),
            vec![dir.path().into()],
            1,
        )
        .unwrap();
        let request = HashRequest {
            root: server.roots()[0].id.clone(),
            rel_path: "file".into(),
            algo: HashAlgo::Blake3,
        };
        let permit = server.permits.clone().acquire_owned().await.unwrap();
        assert_eq!(
            hash(State(server.clone()), Json(request.clone()))
                .await
                .status(),
            StatusCode::TOO_MANY_REQUESTS
        );
        drop(permit);
        let flag = Arc::new(AtomicBool::new(false));
        {
            let _guard = CancelOnDrop(flag.clone());
            assert_eq!(server.hash(request.clone(), &flag).unwrap().size, 3);
        }
        assert_eq!(
            server.hash(request, &flag).unwrap_err().kind(),
            io::ErrorKind::Interrupted
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn disconnect_cancels_hash_and_releases_worker_slot() {
        use tokio::io::AsyncWriteExt;
        let dir = tempfile::tempdir().unwrap();
        let file = std::fs::File::create(dir.path().join("large")).unwrap();
        file.set_len(16 * 1024 * 1024 * 1024).unwrap();
        let server = HashServer::new(
            "id".into(),
            "test".into(),
            "token".into(),
            vec![dir.path().into()],
            1,
        )
        .unwrap();
        let body = serde_json::to_string(&HashRequest {
            root: server.roots()[0].id.clone(),
            rel_path: "large".into(),
            algo: HashAlgo::Blake3,
        })
        .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let application = router(server.clone());
        let serving =
            tokio::spawn(async move { axum::serve(listener, application).await.unwrap() });
        let mut connection = tokio::net::TcpStream::connect(address).await.unwrap();
        connection.write_all(format!(
            "POST /v1/hash HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer token\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len(),
        ).as_bytes()).await.unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while server.permits.available_permits() != 0 {
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        drop(connection);
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while server.permits.available_permits() != 1 {
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("disconnected client must release the hashing slot promptly");
        serving.abort();
    }

    #[cfg(unix)]
    #[test]
    fn list_reports_files_directories_and_non_regular_entries() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("a/@eaDir")).unwrap();
        std::fs::create_dir(root.path().join("a/sub")).unwrap();
        std::fs::write(root.path().join("a/file.mp4"), b"12345").unwrap();
        std::os::unix::fs::symlink("file.mp4", root.path().join("a/link")).unwrap();
        let server = HashServer::new(
            "id".into(),
            "test".into(),
            "token".into(),
            vec![root.path().into()],
            1,
        )
        .unwrap();
        let listing = server.list(&server.roots()[0].id, "a").unwrap();
        assert_eq!(listing.directories, vec!["sub"]);
        assert_eq!(listing.files.len(), 1);
        assert_eq!(listing.files[0].name, "file.mp4");
        assert_eq!(listing.files[0].size, 5);
        assert!(listing.files[0].modified.is_some());
        assert_eq!(listing.other, vec!["link"]);
        assert!(server.list(&server.roots()[0].id, "../").is_err());
    }

    #[test]
    fn timestamps_are_rfc3339_utc() {
        let stamp = timestamp();
        assert_eq!(stamp.len(), 24, "{stamp}");
        assert!(stamp.starts_with("20") && stamp.ends_with('Z'), "{stamp}");
    }

    #[cfg(unix)]
    #[test]
    fn capability_open_rejects_symlinks_after_validation() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("file"), b"safe").unwrap();
        std::fs::write(outside.path().join("secret"), b"private").unwrap();
        let server = HashServer::new(
            "id".into(),
            "test".into(),
            "token".into(),
            vec![root.path().into()],
            1,
        )
        .unwrap();
        let (root, relative) = server.resolve(&server.roots()[0].id, "file").unwrap();
        std::fs::remove_file(root.canonical.join("file")).unwrap();
        std::os::unix::fs::symlink(outside.path().join("secret"), root.canonical.join("file"))
            .unwrap();
        assert!(root.dir.open(relative).is_err());
    }
}
