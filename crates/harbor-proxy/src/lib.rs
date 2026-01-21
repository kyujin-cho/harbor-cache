//! Harbor Cache Upstream Proxy
//!
//! This crate provides the client for communicating with upstream
//! Harbor registries, handling authentication and artifact fetching.

pub mod client;
pub mod error;

pub use client::{HarborClient, HarborClientConfig};
pub use error::ProxyError;
