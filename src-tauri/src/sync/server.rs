use crate::{
    domain::Computer,
    store::{Peer, Store},
    sync::{
        constant_time_eq, Hello, PullRequest, PullResponse, PushRequest, PushResponse, SyncError,
        MAX_BODY_BYTES, PAGE_SIZE,
    },
};
use axum::{
    body::Body,
    extract::{DefaultBodyLimit, Request, State},
    http::{header, HeaderMap, StatusCode},
    middleware::{self, Next},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use std::{net::SocketAddr, sync::Arc};
use tokio::net::TcpListener;

#[derive(Clone)]
struct ServerState {
    store: Arc<Store>,
    token: String,
}

pub async fn start_server(
    store: Arc<Store>,
    token: String,
    port: u16,
) -> Result<SocketAddr, SyncError> {
    let state = ServerState { store, token };
    let app = Router::new()
        .route("/v1/hello", get(hello))
        .route("/v1/pull", post(pull))
        .route("/v1/push", post(push))
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        .layer(middleware::from_fn_with_state(state.clone(), auth))
        .with_state(state);
    let listener = TcpListener::bind(("0.0.0.0", port)).await?;
    let addr = listener.local_addr()?;
    tokio::spawn(async move {
        if let Err(err) = axum::serve(listener, app).await {
            log::error!("sync server failed: {err}");
        }
    });
    Ok(addr)
}

async fn auth(
    State(state): State<ServerState>,
    headers: HeaderMap,
    req: Request<Body>,
    next: Next,
) -> impl IntoResponse {
    let Some(value) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
    else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let Some(token) = value.strip_prefix("Bearer ") else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if !constant_time_eq(token, &state.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    next.run(req).await
}

async fn hello(State(state): State<ServerState>) -> Result<Json<Hello>, ApiError> {
    Ok(Json(make_hello(&state.store)?))
}

async fn pull(
    State(state): State<ServerState>,
    Json(req): Json<PullRequest>,
) -> Result<Json<PullResponse>, ApiError> {
    let ops = state.store.ops_since(&req.version_vector, PAGE_SIZE)?;
    Ok(Json(PullResponse {
        more: ops.len() == PAGE_SIZE,
        ops,
        version_vector: state.store.version_vector()?,
    }))
}

async fn push(
    State(state): State<ServerState>,
    Json(req): Json<PushRequest>,
) -> Result<Json<PushResponse>, ApiError> {
    let applied = state.store.apply_remote(&req.ops)?;
    Ok(Json(PushResponse { applied }))
}

pub fn make_hello(store: &Store) -> Result<Hello, SyncError> {
    let computer_id = store.computer_id().to_string();
    let computer = store.get::<Computer>(&computer_id)?;
    Ok(Hello {
        computer_id,
        name: computer
            .as_ref()
            .map(|c| c.name.clone())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| gethostname::gethostname().to_string_lossy().into_owned()),
        os: computer
            .as_ref()
            .map(|c| c.os.clone())
            .filter(|os| !os.is_empty())
            .unwrap_or_else(|| std::env::consts::OS.to_string()),
        version_vector: store.version_vector()?,
    })
}

struct ApiError(SyncError);

impl<E> From<E> for ApiError
where
    SyncError: From<E>,
{
    fn from(value: E) -> Self {
        Self(value.into())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let status = match self.0 {
            SyncError::Unauthorized => StatusCode::UNAUTHORIZED,
            SyncError::Address(_) | SyncError::InvalidPeer(_) => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (status, self.0.to_string()).into_response()
    }
}

#[allow(dead_code)]
pub fn peer_for_server(id: String, address: String, token: String) -> Peer {
    Peer {
        id,
        address,
        token,
        ..Default::default()
    }
}
