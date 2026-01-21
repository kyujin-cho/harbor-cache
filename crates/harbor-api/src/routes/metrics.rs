//! Prometheus metrics endpoint

use axum::{extract::State, response::IntoResponse, routing::get, Router};
use std::sync::Arc;

use crate::state::MetricsHandle;

/// Create metrics routes with the Prometheus handle
pub fn routes(handle: Arc<MetricsHandle>) -> Router {
    Router::new()
        .route("/metrics", get(get_metrics))
        .with_state(handle)
}

/// GET /metrics - Prometheus metrics endpoint
async fn get_metrics(State(handle): State<Arc<MetricsHandle>>) -> impl IntoResponse {
    handle.render()
}
