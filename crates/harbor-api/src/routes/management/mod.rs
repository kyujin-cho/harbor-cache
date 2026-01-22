//! Management API routes

use axum::Router;

use crate::state::AppState;

// Submodules
mod auth;
mod cache;
mod config;
mod types;
mod users;

/// Create management API routes
pub fn routes() -> Router<AppState> {
    Router::new()
        .merge(auth::routes())
        .merge(users::routes())
        .merge(cache::routes())
        .merge(config::routes())
}
