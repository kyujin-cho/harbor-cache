//! Harbor Cache Database Layer
//!
//! This crate provides the database abstraction layer for Harbor Cache,
//! using SQLite via sqlx for persistence.

pub mod error;
pub mod models;
pub mod repository;
pub mod utils;

pub use error::DbError;
pub use models::*;
pub use repository::{CacheStats, Database};

/// Re-export sqlx types for convenience
pub use sqlx::SqlitePool;

// Re-export upstream-related types for convenience
pub use models::{
    CacheIsolation, NewUpstream, NewUpstreamRoute, UpdateUpstream, Upstream, UpstreamRoute,
};
