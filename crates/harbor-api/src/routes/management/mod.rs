//! Management API routes
//!
//! This module provides the management API for Harbor Cache,
//! including authentication, user management, cache management,
//! and configuration management.

pub mod auth;
pub mod cache;
pub mod config;
pub mod types;
pub mod users;

use axum::Router;

use crate::state::AppState;

// Re-export commonly used types for external use
#[allow(unused_imports)]
pub use auth::{RequireAdmin, RequireAuth};
#[allow(unused_imports)]
pub use types::*;

/// Create management API routes
pub fn routes() -> Router<AppState> {
    Router::new()
        .merge(auth::routes())
        .merge(users::routes())
        .merge(cache::routes())
        .merge(config::routes())
}
