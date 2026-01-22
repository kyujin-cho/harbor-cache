//! API routes

mod health;
mod management;
pub mod metrics;
mod registry;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use std::sync::Arc;
use tower_http::services::{ServeDir, ServeFile};

use crate::state::{AppState, MetricsHandle};

/// Create the main router
pub fn create_router(state: AppState, metrics_handle: Option<Arc<MetricsHandle>>) -> Router {
    // Create static file service with SPA fallback
    let serve_dir = ServeDir::new("static").not_found_service(ServeFile::new("static/index.html"));

    let mut router = Router::new()
        // Health check
        .merge(health::routes())
        // OCI Distribution API (v2)
        .merge(registry::routes())
        // Management API
        .merge(management::routes())
        .with_state(state)
        // Allow large blob uploads (2GB max)
        .layer(DefaultBodyLimit::max(2 * 1024 * 1024 * 1024));

    // Add metrics endpoint if handle is provided
    if let Some(handle) = metrics_handle {
        router = router.merge(metrics::routes(handle));
    }

    // Serve static files (SPA) - must be last to not interfere with API routes
    router.fallback_service(serve_dir)
}
