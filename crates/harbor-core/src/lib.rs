//! Harbor Cache Core Business Logic
//!
//! This crate provides the core functionality for Harbor Cache,
//! including cache management, eviction policies, and registry protocol handling.

pub mod cache;
pub mod config;
pub mod error;
pub mod registry;
pub mod upstream;

pub use cache::{CacheConfig, CacheManager, EvictionPolicy, spawn_cleanup_task};
pub use config::{
    validate_pattern, validate_project_name, UpstreamConfig, UpstreamConfigProvider,
    UpstreamProjectConfig, UpstreamRouteConfig, MAX_PROJECTS_PER_UPSTREAM,
};
pub use error::CoreError;
pub use registry::RegistryService;
pub use upstream::{UpstreamHealth, UpstreamInfo, UpstreamManager};
