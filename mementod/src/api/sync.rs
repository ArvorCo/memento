use crate::manager::{ImportRequest, MementoManager, SyncResponse};
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use std::sync::Arc;

pub async fn sync(
    State(mgr): State<Arc<MementoManager>>,
    Json(req): Json<ImportRequest>,
) -> Result<Json<SyncResponse>, (StatusCode, String)> {
    mgr.sync(&req)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}
