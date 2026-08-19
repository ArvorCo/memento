//! Server — local Unix socket/Windows named pipe + optional HTTP listener.

use crate::api;
use crate::manager::MementoManager;
use axum::extract::Request;
use axum::http::{header, StatusCode};
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::Router;
use std::path::Path;
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::info;

#[cfg(windows)]
use tracing::warn;

#[cfg(unix)]
use tokio::net::UnixListener;

#[cfg(windows)]
use hyper_util::rt::TokioIo;
#[cfg(windows)]
use hyper_util::service::TowerToHyperService;
#[cfg(windows)]
use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};

#[cfg(unix)]
pub struct LocalListener {
    inner: UnixListener,
    endpoint: String,
}

#[cfg(windows)]
pub struct LocalListener {
    inner: NamedPipeServer,
    endpoint: String,
}

pub fn build_router(state: Arc<MementoManager>) -> Router {
    api::routes(state)
}

pub fn with_http_auth(app: Router, token: Arc<str>) -> Router {
    app.layer(middleware::from_fn(move |req, next| {
        let token = token.clone();
        async move { require_http_auth(req, next, token).await }
    }))
}

pub fn prepare_local_endpoint(data_dir: &Path) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        let path = memento_ipc::unix_socket_path(data_dir);
        if path.exists() {
            std::fs::remove_file(path)?;
        }
    }
    Ok(())
}

#[cfg(unix)]
pub fn bind_local(data_dir: &Path) -> anyhow::Result<LocalListener> {
    let path = memento_ipc::unix_socket_path(data_dir);
    Ok(LocalListener {
        inner: UnixListener::bind(&path)?,
        endpoint: path.display().to_string(),
    })
}

#[cfg(windows)]
pub fn bind_local(data_dir: &Path) -> anyhow::Result<LocalListener> {
    let endpoint = memento_ipc::windows_pipe_name(data_dir);
    let inner = pipe_server(&endpoint, true)?;
    Ok(LocalListener { inner, endpoint })
}

#[cfg(windows)]
fn pipe_server(endpoint: &str, first: bool) -> std::io::Result<NamedPipeServer> {
    let mut options = ServerOptions::new();
    options
        .first_pipe_instance(first)
        .reject_remote_clients(true);
    options.create(endpoint)
}

#[cfg(unix)]
pub async fn serve_local(app: Router, listener: LocalListener) -> anyhow::Result<()> {
    info!("Listening on Unix socket: {}", listener.endpoint);
    axum::serve(listener.inner, app).await?;
    Ok(())
}

#[cfg(windows)]
pub async fn serve_local(app: Router, mut listener: LocalListener) -> anyhow::Result<()> {
    info!("Listening on Windows named pipe: {}", listener.endpoint);
    loop {
        listener.inner.connect().await?;
        let connected = listener.inner;
        listener.inner = pipe_server(&listener.endpoint, false)?;
        let service = TowerToHyperService::new(app.clone());
        tokio::spawn(async move {
            let io = TokioIo::new(connected);
            if let Err(error) = hyper::server::conn::http1::Builder::new()
                .serve_connection(io, service)
                .await
            {
                warn!("Named pipe connection error: {error}");
            }
        });
    }
}

pub async fn bind_http(host: &str, port: u16) -> anyhow::Result<TcpListener> {
    Ok(TcpListener::bind(format!("{host}:{port}")).await?)
}

pub async fn serve_http(app: Router, listener: TcpListener, addr: String) -> anyhow::Result<()> {
    info!("Listening on HTTP: {addr}");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn require_http_auth(
    req: Request,
    next: Next,
    token: Arc<str>,
) -> Result<Response, StatusCode> {
    if req.uri().path() == "/health" {
        return Ok(next.run(req).await);
    }

    let authorized = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(|value| value == token.as_ref())
        .unwrap_or(false)
        || req
            .headers()
            .get("x-memento-token")
            .and_then(|value| value.to_str().ok())
            .map(|value| value == token.as_ref())
            .unwrap_or(false);

    if authorized {
        Ok(next.run(req).await)
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request as HttpRequest;
    use axum::routing::get;
    use tower::ServiceExt;

    #[tokio::test]
    async fn http_auth_allows_health_without_token() {
        let app = Router::new()
            .route("/health", get(|| async { "ok" }))
            .route("/status", get(|| async { "secure" }));
        let app = with_http_auth(app, Arc::<str>::from("secret-token"));

        let response = app
            .oneshot(
                HttpRequest::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn http_auth_rejects_missing_token() {
        let app = Router::new().route("/status", get(|| async { "secure" }));
        let app = with_http_auth(app, Arc::<str>::from("secret-token"));

        let response = app
            .oneshot(
                HttpRequest::builder()
                    .uri("/status")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn http_auth_accepts_bearer_token() {
        let app = Router::new().route("/status", get(|| async { "secure" }));
        let app = with_http_auth(app, Arc::<str>::from("secret-token"));

        let response = app
            .oneshot(
                HttpRequest::builder()
                    .uri("/status")
                    .header(header::AUTHORIZATION, "Bearer secret-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }
}
