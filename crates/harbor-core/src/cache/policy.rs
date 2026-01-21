//! Cache eviction policies

use serde::{Deserialize, Serialize};

/// Eviction policy for cache management
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EvictionPolicy {
    /// Least Recently Used - evict items that haven't been accessed recently
    Lru,
    /// Least Frequently Used - evict items with the lowest access count
    Lfu,
    /// First In First Out - evict oldest items first
    Fifo,
}

impl Default for EvictionPolicy {
    fn default() -> Self {
        Self::Lru
    }
}

impl EvictionPolicy {
    pub fn as_str(&self) -> &'static str {
        match self {
            EvictionPolicy::Lru => "lru",
            EvictionPolicy::Lfu => "lfu",
            EvictionPolicy::Fifo => "fifo",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "lru" => Some(EvictionPolicy::Lru),
            "lfu" => Some(EvictionPolicy::Lfu),
            "fifo" => Some(EvictionPolicy::Fifo),
            _ => None,
        }
    }
}
