//! Cache management module

mod manager;
mod policy;

pub use manager::{CacheConfig, CacheManager};
pub use policy::EvictionPolicy;
