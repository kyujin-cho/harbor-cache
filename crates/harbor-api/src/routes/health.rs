//! Health check endpoints

use axum::{Json, Router, routing::get};
use serde::Serialize;

use crate::state::AppState;

/// Health status response
#[derive(Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
}

/// Health check handler
async fn health() -> Json<HealthResponse> {
    // Record health check metric
    metrics::counter!("harbor_cache_health_checks_total").increment(1);

    Json(HealthResponse {
        status: "healthy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

/// Create health routes
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/health", get(health))
        .route("/healthz", get(health))
}
