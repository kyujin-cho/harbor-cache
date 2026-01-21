//! Application state

use harbor_auth::JwtManager;
use harbor_core::{CacheManager, RegistryService};
use harbor_db::Database;
use harbor_storage::StorageBackend;
use std::sync::Arc;

/// Application state shared across handlers
#[derive(Clone)]
pub struct AppState {
    pub db: Database,
    pub cache: Arc<CacheManager>,
    pub registry: Arc<RegistryService>,
    pub storage: Arc<dyn StorageBackend>,
    pub jwt: Arc<JwtManager>,
    pub auth_enabled: bool,
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
        }
    }
}
