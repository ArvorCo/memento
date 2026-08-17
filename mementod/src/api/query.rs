use crate::manager::{MementoManager, QueryRequest, QueryResponse};
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use std::sync::Arc;

/// Original POST /query handler (internal / tool use)
pub async fn query_post(
    State(mgr): State<Arc<MementoManager>>,
    Json(req): Json<QueryRequest>,
) -> Result<Json<QueryResponse>, (StatusCode, String)> {
    mgr.query(&req)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

/// GET /query?q=...&session_id=...&limit=N
/// Aginus-compatible thin shim over the same manager query.
#[derive(Debug, Deserialize)]
pub struct QueryParams {
    pub q: String,
    #[serde(rename = "session_id")]
    pub _session_id: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, serde::Serialize)]
pub struct MemoryChunk {
    pub text: String,
    pub score: f32,
    pub source: String,
}

pub async fn query_get(
    State(mgr): State<Arc<MementoManager>>,
    Query(params): Query<QueryParams>,
) -> Result<Json<Vec<MemoryChunk>>, (StatusCode, String)> {
    let top_k = params.limit.unwrap_or(6);
    let req = QueryRequest {
        query: params.q,
        top_k,
    };
    let response = mgr
        .query(&req)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let chunks: Vec<MemoryChunk> = response
        .results
        .into_iter()
        .map(|r| MemoryChunk {
            text: r.content,
            score: r.score as f32,
            source: r.source_path,
        })
        .collect();

    Ok(Json(chunks))
}
