use axum::Json;
use serde::Serialize;

#[derive(Serialize)]
pub struct UsageResponse {
    queries_this_month: u64,
    imports_this_month: u64,
    storage_mb: f64,
    plan: &'static str,
    message: &'static str,
}

/// GET /v1/usage — placeholder; full implementation in Phase 2
pub async fn get_usage() -> Json<UsageResponse> {
    Json(UsageResponse {
        queries_this_month: 0,
        imports_this_month: 0,
        storage_mb: 0.0,
        plan: "local",
        message: "Usage tracking coming in Phase 2.",
    })
}
