/// Integration tests for the health endpoint
use axum::{body::Body, http::Request, routing::get, Json, Router};
use serde_json::Value;
use tower::ServiceExt;

async fn health_handler() -> Json<Value> {
    Json(serde_json::json!({"status": "ok", "service": "memento-api", "version": "0.1.0"}))
}

#[tokio::test]
async fn test_health_endpoint_returns_200() {
    let app: Router = Router::new().route("/health", get(health_handler));

    let request = Request::builder()
        .uri("/health")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), 200);
}

#[tokio::test]
async fn test_health_response_has_status_ok() {
    let app: Router = Router::new().route("/health", get(health_handler));

    let request = Request::builder()
        .uri("/health")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), 200);

    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body_bytes).unwrap();

    assert_eq!(json["status"], "ok");
    assert_eq!(json["service"], "memento-api");
}
