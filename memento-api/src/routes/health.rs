use axum::Json;
use serde::Serialize;

#[derive(Serialize)]
pub struct HealthResponse {
    status: &'static str,
    version: &'static str,
    service: &'static str,
}

/// GET /health — returns 200 OK with service info
pub async fn health_check() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
        service: "memento-api",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_health_check_returns_ok() {
        let response = health_check().await;
        let body = response.0;
        assert_eq!(body.status, "ok");
        assert_eq!(body.service, "memento-api");
    }
}
