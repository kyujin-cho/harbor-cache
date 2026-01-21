//! API routes

mod health;
mod management;
mod registry;

use axum::Router;

use crate::state::AppState;

/// Create the main router
pub fn create_router(state: AppState) -> Router {
    Router::new()
        // Health check
        .merge(health::routes())
        // OCI Distribution API (v2)
        .merge(registry::routes())
        // Management API
        .merge(management::routes())
        .with_state(state)
}
