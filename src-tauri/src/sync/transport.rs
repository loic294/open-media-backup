use crate::{
    store::{Op, Peer, VersionVector},
    sync::{Hello, PullRequest, PullResponse, PushRequest, PushResponse, SyncError, SyncResult},
};
use reqwest::StatusCode;
use std::{future::Future, pin::Pin, time::Duration};

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub trait SyncTransport: Send + Sync {
    fn hello<'a>(&'a self, peer: &'a Peer) -> BoxFuture<'a, SyncResult<Hello>>;
    fn pull<'a>(
        &'a self,
        peer: &'a Peer,
        version_vector: VersionVector,
    ) -> BoxFuture<'a, SyncResult<PullResponse>>;
    fn push<'a>(&'a self, peer: &'a Peer, ops: Vec<Op>) -> BoxFuture<'a, SyncResult<PushResponse>>;
}

#[derive(Clone)]
pub struct HttpTransport {
    client: reqwest::Client,
}

impl HttpTransport {
    pub fn new() -> SyncResult<Self> {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(30))
            .build()?;
        Ok(Self { client })
    }

    fn url(peer: &Peer, path: &str) -> String {
        format!(
            "http://{}/{}",
            peer.address.trim_end_matches('/'),
            path.trim_start_matches('/')
        )
    }

    async fn send<T: serde::de::DeserializeOwned>(
        &self,
        req: reqwest::RequestBuilder,
    ) -> SyncResult<T> {
        let response = req.send().await.map_err(map_reqwest_error)?;
        if response.status() == StatusCode::UNAUTHORIZED {
            return Err(SyncError::Unauthorized);
        }
        if !response.status().is_success() {
            return Err(SyncError::Other(format!("HTTP {}", response.status())));
        }
        response.json().await.map_err(Into::into)
    }
}

impl Default for HttpTransport {
    fn default() -> Self {
        Self::new().expect("reqwest client should build")
    }
}

impl SyncTransport for HttpTransport {
    fn hello<'a>(&'a self, peer: &'a Peer) -> BoxFuture<'a, SyncResult<Hello>> {
        Box::pin(async move {
            self.send(
                self.client
                    .get(Self::url(peer, "/v1/hello"))
                    .bearer_auth(&peer.token),
            )
            .await
        })
    }

    fn pull<'a>(
        &'a self,
        peer: &'a Peer,
        version_vector: VersionVector,
    ) -> BoxFuture<'a, SyncResult<PullResponse>> {
        Box::pin(async move {
            self.send(
                self.client
                    .post(Self::url(peer, "/v1/pull"))
                    .bearer_auth(&peer.token)
                    .json(&PullRequest { version_vector }),
            )
            .await
        })
    }

    fn push<'a>(&'a self, peer: &'a Peer, ops: Vec<Op>) -> BoxFuture<'a, SyncResult<PushResponse>> {
        Box::pin(async move {
            self.send(
                self.client
                    .post(Self::url(peer, "/v1/push"))
                    .bearer_auth(&peer.token)
                    .json(&PushRequest { ops }),
            )
            .await
        })
    }
}

fn map_reqwest_error(err: reqwest::Error) -> SyncError {
    if err.is_connect() || err.is_timeout() {
        SyncError::Offline(err.to_string())
    } else {
        SyncError::Http(err)
    }
}
