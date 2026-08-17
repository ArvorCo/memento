use axum::Json;
use serde_json::{json, Value};

/// Health endpoint.
/// Returns both `ok` (Aginus-compatible) and `status` (legacy) fields.
pub async fn health() -> Json<Value> {
    Json(json!({ "ok": true, "status": "ok", "service": "mementod" }))
}
