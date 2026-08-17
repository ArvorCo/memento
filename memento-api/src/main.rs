use anyhow::Result;
use axum::{routing::get, Router};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use tower_http::cors::CorsLayer;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod auth;
mod queue;
mod routes;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "memento_api=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let bind_addr = resolve_bind_addr()?;

    let app = Router::new()
        .route("/health", get(routes::health::health_check))
        .route("/v1/memories", get(routes::memories::list_memories))
        .route("/v1/usage", get(routes::usage::get_usage));

    let app = if env_flag("MEMENTO_API_ALLOW_ANY_ORIGIN") {
        app.layer(CorsLayer::permissive())
    } else {
        app
    };

    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    info!("Memento API listening on http://{bind_addr}");
    axum::serve(listener, app).await?;

    Ok(())
}

fn resolve_bind_addr() -> Result<SocketAddr> {
    let port = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(8080);

    let host = std::env::var("MEMENTO_API_HOST")
        .ok()
        .and_then(|raw| raw.parse::<IpAddr>().ok())
        .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));

    if !host.is_loopback() && !env_flag("MEMENTO_API_ALLOW_REMOTE") {
        anyhow::bail!(
            "refusing to bind Memento API to non-loopback host {host}; set MEMENTO_API_ALLOW_REMOTE=1 if you really want remote access"
        );
    }

    Ok(SocketAddr::new(host, port))
}

fn env_flag(key: &str) -> bool {
    matches!(
        std::env::var(key).ok().as_deref(),
        Some("1" | "true" | "TRUE" | "yes" | "YES" | "on" | "ON")
    )
}
