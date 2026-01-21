//! Cache management module

mod manager;
mod policy;

pub use manager::{spawn_cleanup_task, CacheConfig, CacheManager};
pub use policy::EvictionPolicy;
