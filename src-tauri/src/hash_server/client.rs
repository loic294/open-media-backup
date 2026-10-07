use super::{Browse, HashRequest, HashResponse, Hello, Listing, Root};
use crate::store::HashServer;
use std::time::Duration;

const BUSY_RETRIES: u32 = 40;
const BUSY_RETRY_DELAY: Duration = Duration::from_millis(250);

#[derive(Debug)]
pub enum ListError {
    NotFound(String),
    Other(String),
}

#[derive(Clone)]
pub struct Client {
    client: reqwest::Client,
    address: String,
    token: String,
}

impl Client {
    pub fn new(server: &HashServer) -> Result<Self, String> {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(3))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self {
            client,
            address: server.address.clone(),
            token: server.token.clone(),
        })
    }

    fn url(&self, path: &str) -> String {
        format!("http://{}/v1/{path}", self.address)
    }

    async fn send<T: serde::de::DeserializeOwned>(
        &self,
        request: reqwest::RequestBuilder,
    ) -> Result<T, String> {
        self.send_status(request).await.map_err(|(_, error)| error)
    }

    /// Retries briefly while the server's bounded worker pool is busy (HTTP 429).
    async fn send_status<T: serde::de::DeserializeOwned>(
        &self,
        request: reqwest::RequestBuilder,
    ) -> Result<T, (Option<reqwest::StatusCode>, String)> {
        let mut attempt = 0;
        let response = loop {
            let pending = request
                .try_clone()
                .ok_or((None, "hash server request failed".to_string()))?;
            let response = pending.bearer_auth(&self.token).send().await.map_err(|e| {
                let message = if e.is_timeout() {
                    "hash server timed out"
                } else if e.is_connect() {
                    "hash server is unreachable"
                } else {
                    "hash server request failed"
                };
                (None, message.to_string())
            })?;
            if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS && attempt < BUSY_RETRIES
            {
                attempt += 1;
                tokio::time::sleep(BUSY_RETRY_DELAY).await;
                continue;
            }
            break response;
        };
        let status = response.status();
        if !status.is_success() {
            return Err((Some(status), format!("hash server returned HTTP {status}")));
        }
        response
            .json()
            .await
            .map_err(|_| (Some(status), "invalid hash server response".into()))
    }

    fn root_path(root: &str, path: &str) -> Result<(String, String), String> {
        let (id, subfolder) = root.split_once('/').unwrap_or((root, ""));
        if !id.bytes().all(|b| b.is_ascii_hexdigit()) || id.len() != 64 {
            return Err("invalid exposed root id".into());
        }
        omb_hash::protocol::validate_relative(path).map_err(|e| e.to_string())?;
        omb_hash::protocol::validate_relative(subfolder).map_err(|e| e.to_string())?;
        let path = if subfolder.is_empty() {
            path.to_string()
        } else if path.is_empty() {
            subfolder.to_string()
        } else {
            format!("{subfolder}/{path}")
        };
        Ok((id.to_string(), path))
    }

    pub async fn hello(&self) -> Result<Hello, String> {
        let hello: Hello = self
            .send(
                self.client
                    .get(self.url("hello"))
                    .timeout(Duration::from_secs(10)),
            )
            .await?;
        if hello.kind != "hash_server" || uuid::Uuid::parse_str(&hello.server_id).is_err() {
            return Err("address is not an OMB hash server".into());
        }
        Ok(hello)
    }

    pub async fn roots(&self) -> Result<Vec<Root>, String> {
        self.send(
            self.client
                .get(self.url("roots"))
                .timeout(Duration::from_secs(10)),
        )
        .await
    }

    pub async fn browse(&self, root: &str, path: &str) -> Result<Browse, String> {
        let (id, path) = Self::root_path(root, path)?;
        self.send(
            self.client
                .get(self.url(&format!("roots/{id}/browse")))
                .query(&[("path", path)])
                .timeout(Duration::from_secs(10)),
        )
        .await
    }

    /// Lists one directory. `Ok(None)` means the server does not support listing
    /// (an older server), so callers must fall back to local enumeration.
    pub async fn list(&self, root: &str, path: &str) -> Result<Option<Listing>, ListError> {
        let (id, full) = Self::root_path(root, path).map_err(ListError::Other)?;
        let request = |path: String| {
            self.client
                .get(self.url(&format!("roots/{id}/list")))
                .query(&[("path", path)])
                .timeout(Duration::from_secs(60))
        };
        match self.send_status(request(full)).await {
            Ok(listing) => Ok(Some(listing)),
            Err((Some(reqwest::StatusCode::NOT_FOUND), error)) => {
                // Distinguish a missing folder from a server without the endpoint.
                match self.send_status::<Listing>(request(String::new())).await {
                    Err((Some(reqwest::StatusCode::NOT_FOUND), _)) => Ok(None),
                    _ => Err(ListError::NotFound(error)),
                }
            }
            Err((_, error)) => Err(ListError::Other(error)),
        }
    }

    pub async fn hash(&self, request: &HashRequest, size: u64) -> Result<HashResponse, String> {
        self.send(
            self.client
                .post(self.url("hash"))
                .json(request)
                .timeout(Duration::from_secs(30 + (size / (1024 * 1024)).min(21_600))),
        )
        .await
    }
}
