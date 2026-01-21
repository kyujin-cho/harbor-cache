//! Harbor Cache - Lightweight caching proxy for Harbor container registries

use anyhow::Result;
use clap::Parser;
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::trace::TraceLayer;
use tracing::info;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

mod config;

use config::Config;
use harbor_api::{create_router, AppState, MetricsHandle};
use harbor_auth::JwtManager;
use harbor_core::{spawn_cleanup_task, CacheConfig, CacheManager, EvictionPolicy, RegistryService};
use harbor_db::Database;
use harbor_proxy::{HarborClient, HarborClientConfig};
use harbor_storage::{LocalStorage, S3Config, S3Storage, StorageBackend};

/// Harbor Cache - Lightweight caching proxy for Harbor registries
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Configuration file path
    #[arg(short, long, default_value = "config/default.toml")]
    config: String,

    /// Bind address
    #[arg(long, env = "HARBOR_CACHE_BIND")]
    bind: Option<String>,

    /// Port
    #[arg(short, long, env = "HARBOR_CACHE_PORT")]
    port: Option<u16>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line arguments
    let args = Args::parse();

    // Load configuration
    let config = Config::load(&args.config)?;

    // Initialize logging
    init_logging(&config.logging.level);

    info!("Starting Harbor Cache v{}", env!("CARGO_PKG_VERSION"));

    // Initialize database
    let db_path = format!("sqlite:{}?mode=rwc", config.database.path);
    let db = Database::new(&db_path).await?;

    // Create default admin user if no users exist
    if !db.has_users().await? {
        info!("Creating default admin user");
        let password_hash = harbor_auth::hash_password("admin")?;
        db.insert_user(harbor_db::NewUser {
            username: "admin".to_string(),
            password_hash,
            role: harbor_db::UserRole::Admin,
        })
        .await?;
        info!("Default admin user created (username: admin, password: admin)");
    }

    // Initialize storage backend
    let storage: Arc<dyn StorageBackend> = match config.storage.backend.as_str() {
        "s3" => {
            let s3_config = S3Config {
                bucket: config.storage.s3.bucket.clone().unwrap_or_else(|| "harbor-cache".to_string()),
                region: config.storage.s3.region.clone().unwrap_or_else(|| "us-east-1".to_string()),
                endpoint: config.storage.s3.endpoint.clone(),
                access_key_id: config.storage.s3.access_key.clone(),
                secret_access_key: config.storage.s3.secret_key.clone(),
                prefix: config.storage.s3.prefix.clone(),
                allow_http: config.storage.s3.allow_http,
            };
            info!("Using S3 storage backend: bucket={}", s3_config.bucket);
            Arc::new(S3Storage::new(s3_config).await?)
        }
        _ => {
            // Default to local storage
            tokio::fs::create_dir_all(&config.storage.local.path).await?;
            info!("Using local storage backend: path={}", config.storage.local.path);
            Arc::new(LocalStorage::new(&config.storage.local.path).await?)
        }
    };

    // Initialize upstream client
    let upstream = Arc::new(HarborClient::new(HarborClientConfig {
        url: config.upstream.url.clone(),
        registry: config.upstream.registry.clone(),
        username: config.upstream.username.clone(),
        password: config.upstream.password.clone(),
        skip_tls_verify: config.upstream.skip_tls_verify,
    })?);

    // Initialize cache manager
    let cache_config = CacheConfig {
        max_size: config.cache.max_size,
        retention_days: config.cache.retention_days,
        eviction_policy: EvictionPolicy::from_str(&config.cache.eviction_policy)
            .unwrap_or_default(),
    };
    let cache = Arc::new(CacheManager::new(db.clone(), storage.clone(), cache_config));

    // Spawn background cleanup task (runs every hour)
    let _cleanup_handle = spawn_cleanup_task(cache.clone(), 1);

    // Initialize registry service
    let registry = Arc::new(RegistryService::new(
        cache.clone(),
        upstream,
        db.clone(),
        storage.clone(),
    ));

    // Initialize JWT manager
    let jwt = Arc::new(JwtManager::new(&config.auth.jwt_secret, 24));

    // Create application state
    let state = AppState::new(
        db,
        cache,
        registry,
        storage,
        jwt,
        config.auth.enabled,
    );

    // Initialize Prometheus metrics
    let metrics_handle = init_metrics();

    // Create router
    let app = create_router(state, metrics_handle.map(Arc::new))
        .layer(TraceLayer::new_for_http());

    // Determine bind address
    let bind_addr = args.bind.unwrap_or(config.server.bind_address);
    let port = args.port.unwrap_or(config.server.port);
    let addr: SocketAddr = format!("{}:{}", bind_addr, port).parse()?;

    info!("Listening on {}", addr);
    info!("Upstream: {}", config.upstream.url);

    // Start server
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    info!("Server stopped");
    Ok(())
}

/// Initialize logging
fn init_logging(level: &str) {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(level));

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(filter)
        .init();
}

/// Initialize Prometheus metrics
fn init_metrics() -> Option<MetricsHandle> {
    use metrics_exporter_prometheus::PrometheusBuilder;

    match PrometheusBuilder::new().install_recorder() {
        Ok(handle) => {
            info!("Prometheus metrics enabled at /metrics");

            // Register some default metrics
            metrics::describe_counter!(
                "harbor_cache_requests_total",
                "Total number of cache requests"
            );
            metrics::describe_counter!(
                "harbor_cache_hits_total",
                "Total number of cache hits"
            );
            metrics::describe_counter!(
                "harbor_cache_misses_total",
                "Total number of cache misses"
            );
            metrics::describe_gauge!(
                "harbor_cache_size_bytes",
                "Current cache size in bytes"
            );
            metrics::describe_gauge!(
                "harbor_cache_entries",
                "Current number of cache entries"
            );
            metrics::describe_histogram!(
                "harbor_cache_request_duration_seconds",
                "Request duration in seconds"
            );

            Some(handle)
        }
        Err(e) => {
            tracing::warn!("Failed to initialize Prometheus metrics: {}", e);
            None
        }
    }
}

/// Wait for shutdown signal
async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to install CTRL+C handler");
    info!("Shutdown signal received");
}
