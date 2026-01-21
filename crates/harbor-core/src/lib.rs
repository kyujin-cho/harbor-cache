//! Harbor Cache Core Business Logic
//!
//! This crate provides the core functionality for Harbor Cache,
//! including cache management, eviction policies, and registry protocol handling.

pub mod cache;
pub mod error;
pub mod registry;

pub use cache::{spawn_cleanup_task, CacheConfig, CacheManager, EvictionPolicy};
pub use error::CoreError;
pub use registry::RegistryService;
