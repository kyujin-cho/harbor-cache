//! Application state

use harbor_auth::JwtManager;
use harbor_core::{CacheManager, RegistryService, UpstreamConfigProvider, UpstreamManager};
use harbor_db::Database;
use harbor_storage::StorageBackend;
use std::sync::Arc;

/// Type alias for the Prometheus metrics handle
pub type MetricsHandle = metrics_exporter_prometheus::PrometheusHandle;

/// Application state shared across handlers
#[derive(Clone)]
pub struct AppState {
    pub db: Database,
    pub cache: Arc<CacheManager>,
    pub registry: Arc<RegistryService>,
    pub storage: Arc<dyn StorageBackend>,
    pub jwt: Arc<JwtManager>,
    pub auth_enabled: bool,
    /// Upstream manager for handling multiple registries
    pub upstream_manager: Arc<UpstreamManager>,
    /// Config provider for upstream configuration (TOML-based)
    pub config_provider: Arc<dyn UpstreamConfigProvider>,
}

impl AppState {
    pub fn new(
        db: Database,
        cache: Arc<CacheManager>,
        registry: Arc<RegistryService>,
        storage: Arc<dyn StorageBackend>,
        jwt: Arc<JwtManager>,
        auth_enabled: bool,
        upstream_manager: Arc<UpstreamManager>,
        config_provider: Arc<dyn UpstreamConfigProvider>,
    ) -> Self {
        Self {
            db,
            cache,
            registry,
            storage,
            jwt,
            auth_enabled,
            upstream_manager,
            config_provider,
        }
    }
}
