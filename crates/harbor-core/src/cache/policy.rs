//! Cache eviction policies

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Error type for parsing eviction policy
#[derive(Debug, Clone)]
pub struct ParseEvictionPolicyError(String);

impl fmt::Display for ParseEvictionPolicyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Invalid eviction policy: {}", self.0)
    }
}

impl std::error::Error for ParseEvictionPolicyError {}

/// Eviction policy for cache management
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum EvictionPolicy {
    /// Least Recently Used - evict items that haven't been accessed recently
    #[default]
    Lru,
    /// Least Frequently Used - evict items with the lowest access count
    Lfu,
    /// First In First Out - evict oldest items first
    Fifo,
}

impl EvictionPolicy {
    pub fn as_str(&self) -> &'static str {
        match self {
            EvictionPolicy::Lru => "lru",
            EvictionPolicy::Lfu => "lfu",
            EvictionPolicy::Fifo => "fifo",
        }
    }
}

impl FromStr for EvictionPolicy {
    type Err = ParseEvictionPolicyError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "lru" => Ok(EvictionPolicy::Lru),
            "lfu" => Ok(EvictionPolicy::Lfu),
            "fifo" => Ok(EvictionPolicy::Fifo),
            _ => Err(ParseEvictionPolicyError(s.to_string())),
        }
    }
}
