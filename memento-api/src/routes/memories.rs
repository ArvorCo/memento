use axum::Json;
use serde::Serialize;

#[derive(Serialize)]
pub struct MemoriesResponse {
    memories: Vec<serde_json::Value>,
    total: usize,
    message: &'static str,
}

/// GET /v1/memories — placeholder; full implementation in Phase 2
pub async fn list_memories() -> Json<MemoriesResponse> {
    Json(MemoriesResponse {
        memories: vec![],
        total: 0,
        message: "Remote memories endpoint coming in Phase 2. Use `memento query` locally.",
    })
}
