use crate::manager::MementoManager;
use axum::extract::State;
use axum::Json;
use std::sync::Arc;

pub async fn status(
    State(mgr): State<Arc<MementoManager>>,
) -> Json<crate::manager::StatusResponse> {
    Json(mgr.status().await)
}
