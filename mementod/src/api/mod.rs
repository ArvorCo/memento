//! API routes for mementod

mod document;
mod health;
mod import;
mod ingest;
mod learn;
mod query;
mod status;
mod sync;

use crate::manager::MementoManager;
use axum::routing::{get, post};
use axum::Router;
use std::sync::Arc;

pub fn routes(state: Arc<MementoManager>) -> Router {
    Router::new()
        .route("/health", get(health::health))
        .route("/status", get(status::status))
        .route("/document", post(document::document))
        // Original POST /query (internal / tool use)
        .route("/query", post(query::query_post))
        // GET /query?q=...&session_id=...&limit=N  (Aginus-compatible shim)
        .route("/query", get(query::query_get))
        // POST /ingest — Aginus writes conversation turns into memory
        .route("/ingest", post(ingest::ingest))
        .route("/import", post(import::import))
        .route("/sync", post(sync::sync))
        .route("/learn", post(learn::learn))
        .with_state(state)
}
