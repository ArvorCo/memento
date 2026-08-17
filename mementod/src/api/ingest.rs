/// POST /ingest — Aginus-compatible endpoint to write a conversational turn into memory
use crate::manager::MementoManager;
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct IngestRequest {
    pub text: String,
    pub session_id: Option<String>,
    pub source: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct IngestResponse {
    pub id: String,
}

pub async fn ingest(
    State(mgr): State<Arc<MementoManager>>,
    Json(req): Json<IngestRequest>,
) -> Result<Json<IngestResponse>, (StatusCode, String)> {
    let source = req.source.unwrap_or_else(|| "aginus/telegram".to_string());
    let _session_id = req.session_id.unwrap_or_else(|| "default".to_string());

    // Delegate to the manager's raw-text ingest (file import with inline text)
    mgr.ingest_text(&req.text, &source)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(IngestResponse {
        id: Uuid::new_v4().to_string(),
    }))
}
