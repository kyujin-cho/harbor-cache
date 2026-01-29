//! Application state

use harbor_auth::JwtManager;
use harbor_core::{CacheManager, RegistryService};
use harbor_db::Database;
use harbor_storage::StorageBackend;
use std::sync::Arc;
use tokio::sync::RwLock;

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
    /// Path to the configuration file for reading/writing
    pub config_path: Option<Arc<RwLock<String>>>,
}

impl AppState {
    pub fn new(
        db: Database,
        cache: Arc<CacheManager>,
        registry: Arc<RegistryService>,
        storage: Arc<dyn StorageBackend>,
        jwt: Arc<JwtManager>,
        auth_enabled: bool,
    ) -> Self {
        Self {
            db,
            cache,
            registry,
            storage,
            jwt,
            auth_enabled,
            config_path: None,
        }
    }

    /// Set the configuration file path
    pub fn with_config_path(mut self, path: String) -> Self {
        self.config_path = Some(Arc::new(RwLock::new(path)));
        self
    }
}
