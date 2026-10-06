use super::{Browse, HashRequest, HashResponse, Hello, Root};
use crate::store::HashServer;
use std::time::Duration;

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
        let response = request.bearer_auth(&self.token).send().await.map_err(|e| {
            if e.is_timeout() {
                "hash server timed out".to_string()
            } else if e.is_connect() {
                "hash server is unreachable".to_string()
            } else {
                "hash server request failed".to_string()
            }
        })?;
        if !response.status().is_success() {
            return Err(format!("hash server returned HTTP {}", response.status()));
        }
        response
            .json()
            .await
            .map_err(|_| "invalid hash server response".into())
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
        self.send(
            self.client
                .get(self.url(&format!("roots/{id}/browse")))
                .query(&[("path", path)])
                .timeout(Duration::from_secs(10)),
        )
        .await
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
