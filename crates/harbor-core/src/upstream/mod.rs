//! Upstream management for multi-registry support
//!
//! This module provides the UpstreamManager which handles:
//! - Managing multiple HarborClient instances
//! - Routing requests to appropriate upstreams based on patterns
//! - Health monitoring per upstream
//! - Dynamic upstream configuration

mod manager;
mod router;

pub use manager::{UpstreamInfo, UpstreamManager, UpstreamHealth};
pub use router::RouteMatch;
