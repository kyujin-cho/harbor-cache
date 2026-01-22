//! Cache management module

mod manager;
mod policy;

pub use manager::{CacheConfig, CacheManager, spawn_cleanup_task};
pub use policy::EvictionPolicy;
