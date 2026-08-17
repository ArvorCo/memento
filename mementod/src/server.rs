//! Server — Unix socket + optional HTTP listener

use crate::api;
use crate::manager::MementoManager;
use axum::extract::Request;
use axum::http::{header, StatusCode};
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::Router;
use std::path::Path;
use std::sync::Arc;
use tokio::net::{TcpListener, UnixListener};
use tracing::info;

pub fn build_router(state: Arc<MementoManager>) -> Router {
    api::routes(state)
}

pub fn with_http_auth(app: Router, token: Arc<str>) -> Router {
    app.layer(middleware::from_fn(move |req, next| {
        let token = token.clone();
        async move { require_http_auth(req, next, token).await }
    }))
}

pub fn bind_unix_socket(path: &Path) -> anyhow::Result<UnixListener> {
    Ok(UnixListener::bind(path)?)
}

pub async fn serve_unix_socket(
    app: Router,
    listener: UnixListener,
    path: &Path,
) -> anyhow::Result<()> {
    info!("Listening on Unix socket: {}", path.display());
    axum::serve(listener, app).await?;
    Ok(())
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
