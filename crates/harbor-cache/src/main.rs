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
use harbor_api::{create_router, AppState};
use harbor_auth::JwtManager;
use harbor_core::{CacheConfig, CacheManager, EvictionPolicy, RegistryService};
use harbor_db::Database;
use harbor_proxy::{HarborClient, HarborClientConfig};
use harbor_storage::LocalStorage;

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

    // Create data directories
    tokio::fs::create_dir_all(&config.storage.local.path).await?;

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
    let storage = Arc::new(LocalStorage::new(&config.storage.local.path).await?);

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

    // Create router
    let app = create_router(state)
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

/// Wait for shutdown signal
async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to install CTRL+C handler");
    info!("Shutdown signal received");
}
