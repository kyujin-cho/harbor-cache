//! Harbor Cache REST API
//!
//! This crate provides the Axum-based HTTP API for Harbor Cache,
//! implementing both the OCI Distribution API and the management API.

pub mod error;
pub mod routes;
pub mod state;

pub use error::ApiError;
pub use routes::create_router;
pub use state::{AppState, MetricsHandle};
