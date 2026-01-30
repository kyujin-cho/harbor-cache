//! Application state

use harbor_auth::JwtManager;
use harbor_core::{CacheManager, RegistryService, UpstreamConfigProvider, UpstreamManager};
use harbor_db::Database;
use harbor_storage::StorageBackend;
use std::sync::Arc;

/// Type alias for the Prometheus metrics handle
pub type MetricsHandle = metrics_exporter_prometheus::PrometheusHandle;

/// Minimum allowed TTL for presigned URLs (60 seconds = 1 minute)
const MIN_PRESIGNED_URL_TTL_SECS: u64 = 60;

/// Maximum allowed TTL for presigned URLs (86400 seconds = 24 hours)
const MAX_PRESIGNED_URL_TTL_SECS: u64 = 86400;

/// Blob serving configuration for presigned URL redirects
#[derive(Clone, Debug)]
pub struct BlobServingConfig {
    /// Whether presigned URL redirects are enabled
    pub enable_presigned_redirects: bool,
    /// TTL for presigned URLs in seconds (validated to be within 60-86400)
    pub presigned_url_ttl_secs: u64,
}

impl BlobServingConfig {
    /// Create a new BlobServingConfig with validated TTL.
    /// TTL is clamped to the valid range [60, 86400] seconds.
    pub fn new(enable_presigned_redirects: bool, presigned_url_ttl_secs: u64) -> Self {
        let validated_ttl =
            presigned_url_ttl_secs.clamp(MIN_PRESIGNED_URL_TTL_SECS, MAX_PRESIGNED_URL_TTL_SECS);
        Self {
            enable_presigned_redirects,
            presigned_url_ttl_secs: validated_ttl,
        }
    }
}

impl Default for BlobServingConfig {
    fn default() -> Self {
        Self {
            enable_presigned_redirects: false,
            presigned_url_ttl_secs: 900, // 15 minutes
        }
    }
}

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
    /// Blob serving configuration (presigned URL redirects)
    pub blob_serving: BlobServingConfig,
}

impl AppState {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        db: Database,
        cache: Arc<CacheManager>,
        registry: Arc<RegistryService>,
        storage: Arc<dyn StorageBackend>,
        jwt: Arc<JwtManager>,
        auth_enabled: bool,
        upstream_manager: Arc<UpstreamManager>,
        config_provider: Arc<dyn UpstreamConfigProvider>,
        blob_serving: BlobServingConfig,
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
            blob_serving,
        }
    }
}
