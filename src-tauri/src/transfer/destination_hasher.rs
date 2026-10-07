use super::{
    copy::{hash_checked_with_progress, CopyError},
    handle::JobHandle,
    AnalysisPhase,
};
use crate::{
    domain::{HashAlgo, RemoteHash},
    hash_server::{Client, HashRequest, HashResponse, ListError, Listing},
    paths::to_relative,
    store::Store,
};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Default)]
pub enum DestinationHasher {
    #[default]
    Local,
    Remote {
        client: Client,
        root: String,
        dest_root: PathBuf,
    },
    Unavailable(String),
}

impl DestinationHasher {
    pub fn resolve(
        store: &Store,
        mapping: Option<&RemoteHash>,
        dest_root: &Path,
    ) -> Result<Self, String> {
        let Some(mapping) = mapping.filter(|m| m.enabled) else {
            return Ok(Self::Local);
        };
        let server = store
            .hash_servers()
            .map_err(|e| e.to_string())?
            .into_iter()
            .find(|server| server.id == mapping.server_id);
        let Some(server) = server else {
            return Ok(Self::Unavailable(
                "hash server is not available on this computer".into(),
            ));
        };
        match Client::new(&server) {
            Ok(client) => Ok(Self::Remote {
                client,
                root: mapping.root.clone(),
                dest_root: dest_root.into(),
            }),
            Err(error) => Ok(Self::Unavailable(error)),
        }
    }

    pub fn is_remote(&self) -> bool {
        matches!(self, Self::Remote { .. })
    }

    /// Lists one destination folder on the hash server. `Ok(None)` means the
    /// hasher is local or the server predates listing support.
    pub fn list(
        &self,
        relative: &str,
        handle: &JobHandle,
    ) -> Result<Option<Result<Listing, ListError>>, CopyError> {
        let Self::Remote { client, root, .. } = self else {
            return Ok(None);
        };
        match block_on_cancellable(handle, client.list(root, relative))? {
            Ok(Some(listing)) => Ok(Some(Ok(listing))),
            Ok(None) => Ok(None),
            Err(error) => Ok(Some(Err(error))),
        }
    }

    /// Hashes a file found by remote listing without re-reading local metadata over
    /// the network share. The caller compares the response with the listed entry.
    pub fn hash_listed(
        &self,
        relative: &str,
        algo: HashAlgo,
        size: u64,
        handle: &JobHandle,
    ) -> Result<Result<HashResponse, String>, CopyError> {
        let Self::Remote { client, root, .. } = self else {
            return Ok(Err("hash server is not configured".into()));
        };
        let _phase = handle.analysis_phase(AnalysisPhase::RemoteCheck);
        let request = HashRequest {
            root: root.clone(),
            rel_path: relative.into(),
            algo,
        };
        let response = block_on_cancellable(handle, client.hash(&request, size))?;
        Ok(response.and_then(|response| {
            validate_response(&response, algo, size)?;
            handle.analysis_metrics(|m| m.remote_check_bytes += response.size);
            Ok(response)
        }))
    }

    pub fn hash(
        &self,
        path: &Path,
        algo: HashAlgo,
        handle: &JobHandle,
        mut on_bytes: impl FnMut(u64, u64),
    ) -> Result<String, CopyError> {
        handle.checkpoint()?;
        let result: Result<(), String> = match self {
            Self::Local => return hash_checked_with_progress(path, algo, handle, false, on_bytes),
            Self::Unavailable(error) => Err(error.clone()),
            Self::Remote {
                client,
                root,
                dest_root,
            } => {
                let _phase = handle.analysis_phase(AnalysisPhase::RemoteCheck);
                let before = fs::metadata(path)?;
                let relative = to_relative(dest_root, path).ok_or_else(|| {
                    std::io::Error::new(
                        std::io::ErrorKind::PermissionDenied,
                        "hash target is outside destination device root",
                    )
                })?;
                let request = HashRequest {
                    root: root.clone(),
                    rel_path: relative,
                    algo,
                };
                let response = block_on_cancellable(handle, client.hash(&request, before.len()))?;
                handle.checkpoint()?;
                match response {
                    Ok(response) => {
                        let after = fs::metadata(path)?;
                        match validate_response(&response, algo, before.len()) {
                            Ok(())
                                if before.len() == after.len()
                                    && before.modified()? == after.modified()? =>
                            {
                                handle.analysis_metrics(|m| m.remote_check_bytes += response.size);
                                on_bytes(response.size, response.size);
                                handle.checkpoint()?;
                                return Ok(response.hash);
                            }
                            Ok(()) => Err("destination changed during remote check".into()),
                            Err(error) => Err(error),
                        }
                    }
                    Err(error) => Err(error),
                }
            }
        };
        if let Err(error) = result {
            warn_fallback(handle, &error);
        }
        hash_checked_with_progress(path, algo, handle, false, on_bytes)
    }
}

/// One warning per reason per job, rather than an unbounded warning per file.
pub(crate) fn warn_fallback(handle: &JobHandle, error: &str) {
    let warning = format!("Remote hash check fell back to local re-read: {error}");
    handle.update(|job| {
        if !job.warnings.contains(&warning) {
            job.warnings.push(warning);
        }
    });
}

/// Runs a hash-server request while still honouring pause and cancel.
fn block_on_cancellable<T>(
    handle: &JobHandle,
    future: impl std::future::Future<Output = T>,
) -> Result<T, CopyError> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        tokio::pin!(future);
        let mut poll = tokio::time::interval(Duration::from_millis(50));
        loop {
            tokio::select! {
                result = &mut future => break Ok(result),
                _ = poll.tick() => { handle.checkpoint()?; }
            }
        }
    })
}

pub(crate) fn validate_response(
    response: &HashResponse,
    algo: HashAlgo,
    size: u64,
) -> Result<(), String> {
    if response.size != size {
        return Err("remote file size does not match local file".into());
    }
    let expected = match algo {
        HashAlgo::Blake3 => 64,
        HashAlgo::Xxh64 => 16,
    };
    if response.hash.len() != expected
        || !response
            .hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("hash server returned an invalid digest".into());
    }
    Ok(())
}
