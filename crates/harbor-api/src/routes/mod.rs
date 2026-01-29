//! API routes

mod health;
mod management;
pub mod metrics;
mod registry;

use axum::{
    Router,
    extract::DefaultBodyLimit,
    http::{StatusCode, Uri, header},
    response::{Html, IntoResponse, Response},
};
use rust_embed::Embed;
use std::sync::Arc;

use crate::state::{AppState, MetricsHandle};

/// Embedded static files from the frontend build
#[derive(Embed)]
#[folder = "$CARGO_MANIFEST_DIR/../../static"]
struct Assets;

/// Handler for serving embedded static files
async fn serve_embedded_file(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');

    // Try to get the exact file
    if let Some(content) = <Assets as Embed>::get(path) {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        (
            [(header::CONTENT_TYPE, mime.as_ref())],
            content.data.into_owned(),
        )
            .into_response()
    } else if let Some(content) = <Assets as Embed>::get("index.html") {
        // SPA fallback: serve index.html for any unmatched route
        Html(content.data.into_owned()).into_response()
    } else {
        StatusCode::NOT_FOUND.into_response()
    }
}

/// Create the main router
pub fn create_router(state: AppState, metrics_handle: Option<Arc<MetricsHandle>>) -> Router {
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

    // Serve embedded static files (SPA) - must be last to not interfere with API routes
    router.fallback(serve_embedded_file)
}
