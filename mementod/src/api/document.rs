use crate::manager::{DocumentRequest, DocumentResponse, MementoManager};
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use std::sync::Arc;

pub async fn document(
    State(manager): State<Arc<MementoManager>>,
    Json(request): Json<DocumentRequest>,
) -> Result<Json<DocumentResponse>, (StatusCode, String)> {
    manager
        .get_document(&request)
        .await
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?
        .map(Json)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                "document is not present in Memento; search first and pass an exact source_path"
                    .to_string(),
            )
        })
}
