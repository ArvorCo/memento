use crate::manager::{ImportRequest, ImportResponse, MementoManager};
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use std::sync::Arc;

pub async fn import(
    State(mgr): State<Arc<MementoManager>>,
    Json(req): Json<ImportRequest>,
) -> Result<Json<ImportResponse>, (StatusCode, String)> {
    mgr.import(&req)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}
