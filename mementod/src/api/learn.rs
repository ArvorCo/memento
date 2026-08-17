use crate::manager::{LearnResponse, MementoManager};
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use std::sync::Arc;

pub async fn learn(
    State(mgr): State<Arc<MementoManager>>,
) -> Result<Json<LearnResponse>, (StatusCode, String)> {
    mgr.learn()
        .await
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}
