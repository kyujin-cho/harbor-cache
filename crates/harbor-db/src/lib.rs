//! Harbor Cache Database Layer
//!
//! This crate provides the database abstraction layer for Harbor Cache,
//! using SQLite via sqlx for persistence.

pub mod error;
pub mod models;
pub mod repository;

pub use error::DbError;
pub use models::*;
pub use repository::{CacheStats, Database};

/// Re-export sqlx types for convenience
pub use sqlx::SqlitePool;
