//! Harbor Cache - Lightweight caching proxy for Harbor container registries

use anyhow::{Context, Result};
use clap::Parser;
use std::fs::File;
use std::io::BufReader;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio_rustls::TlsAcceptor;
use tokio_rustls::rustls::ServerConfig as RustlsServerConfig;
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer};
use tower::Service;
use tower_http::trace::TraceLayer;
use tracing::info;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

mod config;

use config::{Config, ConfigManager, UpstreamConfig};
use harbor_api::{AppState, BlobServingConfig, MetricsHandle, create_router};
use harbor_auth::JwtManager;
use harbor_core::config::UpstreamConfigProvider;
use harbor_core::{
    CacheConfig, CacheManager, RegistryService, UpstreamManager, spawn_cleanup_task,
};
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

/// Adapter to make ConfigManager implement UpstreamConfigProvider
struct ConfigManagerAdapter {
    manager: ConfigManager,
}

impl ConfigManagerAdapter {
    fn new(manager: ConfigManager) -> Self {
        Self { manager }
    }
}

impl UpstreamConfigProvider for ConfigManagerAdapter {
    fn get_upstreams(&self) -> Vec<harbor_core::UpstreamConfig> {
        self.manager
            .get_upstreams()
            .into_iter()
            .map(|u| config_to_core_upstream(&u))
            .collect()
    }

    fn get_upstream_by_name(&self, name: &str) -> Option<harbor_core::UpstreamConfig> {
        self.manager
            .get_upstream_by_name(name)
            .map(|u| config_to_core_upstream(&u))
    }

    fn get_default_upstream(&self) -> Option<harbor_core::UpstreamConfig> {
        self.manager
            .get_default_upstream()
            .map(|u| config_to_core_upstream(&u))
    }

    fn add_upstream(&self, upstream: harbor_core::UpstreamConfig) -> anyhow::Result<()> {
        let config_upstream = core_to_config_upstream(&upstream);
        self.manager.add_upstream(config_upstream)
    }

    fn update_upstream(
        &self,
        name: &str,
        updated: harbor_core::UpstreamConfig,
    ) -> anyhow::Result<()> {
        let config_upstream = core_to_config_upstream(&updated);
        self.manager.update_upstream(name, config_upstream)
    }

    fn remove_upstream(&self, name: &str) -> anyhow::Result<harbor_core::UpstreamConfig> {
        let removed = self.manager.remove_upstream(name)?;
        Ok(config_to_core_upstream(&removed))
    }

    fn get_config_path(&self) -> String {
        self.manager.get_path()
    }
}

/// Convert config::UpstreamConfig to harbor_core::UpstreamConfig
fn config_to_core_upstream(config: &UpstreamConfig) -> harbor_core::UpstreamConfig {
    harbor_core::UpstreamConfig {
        name: config.name.clone(),
        display_name: config.display_name.clone(),
        url: config.url.clone(),
        registry: config.registry.clone(),
        projects: config
            .projects
            .iter()
            .map(|p| harbor_core::UpstreamProjectConfig {
                name: p.name.clone(),
                pattern: p.pattern.clone(),
                priority: p.priority,
                is_default: p.is_default,
            })
            .collect(),
        username: config.username.clone(),
        password: config.password.clone(),
        skip_tls_verify: config.skip_tls_verify,
        priority: config.priority,
        enabled: config.enabled,
        cache_isolation: config.cache_isolation.clone(),
        is_default: config.is_default,
        routes: config
            .routes
            .iter()
            .map(|r| harbor_core::UpstreamRouteConfig {
                pattern: r.pattern.clone(),
                priority: r.priority,
            })
            .collect(),
    }
}

/// Convert harbor_core::UpstreamConfig to config::UpstreamConfig
fn core_to_config_upstream(core: &harbor_core::UpstreamConfig) -> UpstreamConfig {
    UpstreamConfig {
        name: core.name.clone(),
        display_name: core.display_name.clone(),
        url: core.url.clone(),
        registry: core.registry.clone(),
        projects: core
            .projects
            .iter()
            .map(|p| config::UpstreamProjectConfig {
                name: p.name.clone(),
                pattern: p.pattern.clone(),
                priority: p.priority,
                is_default: p.is_default,
            })
            .collect(),
        username: core.username.clone(),
        password: core.password.clone(),
        skip_tls_verify: core.skip_tls_verify,
        priority: core.priority,
        enabled: core.enabled,
        cache_isolation: core.cache_isolation.clone(),
        is_default: core.is_default,
        routes: core
            .routes
            .iter()
            .map(|r| config::UpstreamRouteConfig {
                pattern: r.pattern.clone(),
                priority: r.priority,
            })
            .collect(),
    }
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
    let db_file_path = std::path::Path::new(&config.database.path);
    if let Some(parent) = db_file_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
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
                bucket: config
                    .storage
                    .s3
                    .bucket
                    .clone()
                    .unwrap_or_else(|| "harbor-cache".to_string()),
                region: config
                    .storage
                    .s3
                    .region
                    .clone()
                    .unwrap_or_else(|| "us-east-1".to_string()),
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
            info!(
                "Using local storage backend: path={}",
                config.storage.local.path
            );
            Arc::new(LocalStorage::new(&config.storage.local.path).await?)
        }
    };

    // Create config manager for runtime updates
    let config_manager = ConfigManager::new(config.clone(), args.config.clone());
    let config_provider: Arc<dyn UpstreamConfigProvider> =
        Arc::new(ConfigManagerAdapter::new(config_manager.clone()));

    // Initialize upstream manager with config provider
    let upstream_manager = Arc::new(
        UpstreamManager::new(config_provider.clone())
            .context("Failed to initialize upstream manager")?,
    );

    // Get the default upstream for the legacy RegistryService
    // For compatibility, we still need a single HarborClient for RegistryService
    let default_upstream = config
        .get_default_upstream()
        .ok_or_else(|| anyhow::anyhow!("No default upstream configured"))?;

    let upstream = Arc::new(HarborClient::new(HarborClientConfig {
        url: default_upstream.url.clone(),
        registry: default_upstream.registry.clone(),
        username: default_upstream.username.clone(),
        password: default_upstream.password.clone(),
        skip_tls_verify: default_upstream.skip_tls_verify,
    })?);

    info!(
        "Default upstream: {} -> {}",
        default_upstream.name, default_upstream.url
    );

    // Initialize cache manager
    let cache_config = CacheConfig {
        max_size: config.cache.max_size,
        retention_days: config.cache.retention_days,
        eviction_policy: config.cache.eviction_policy.parse().unwrap_or_default(),
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

    // Configure blob serving (presigned URL redirects)
    let blob_serving = BlobServingConfig {
        enable_presigned_redirects: config.blob_serving.enable_presigned_redirects,
        presigned_url_ttl_secs: config.blob_serving.presigned_url_ttl_secs,
    };

    if blob_serving.enable_presigned_redirects {
        info!(
            "Presigned URL redirects enabled (TTL: {}s)",
            blob_serving.presigned_url_ttl_secs
        );
    }

    // Create application state
    let state = AppState::new(
        db,
        cache,
        registry,
        storage,
        jwt,
        config.auth.enabled,
        upstream_manager,
        config_provider,
        blob_serving,
    );

    // Initialize Prometheus metrics
    let metrics_handle = init_metrics();

    // Create router
    let app = create_router(state, metrics_handle.map(Arc::new)).layer(TraceLayer::new_for_http());

    // Determine bind address
    let bind_addr = args.bind.unwrap_or(config.server.bind_address.clone());
    let port = args.port.unwrap_or(config.server.port);
    let addr: SocketAddr = format!("{}:{}", bind_addr, port).parse()?;

    // Log all configured upstreams
    info!("Configured upstreams:");
    for upstream in config.get_upstreams() {
        info!(
            "  - {} ({}): {} [{}]",
            upstream.name,
            upstream.display_name(),
            upstream.url,
            if upstream.is_default {
                "default"
            } else {
                "active"
            }
        );
    }

    // Start server with or without TLS
    if config.tls.enabled {
        let tls_config = load_tls_config(&config.tls)?;
        let tls_acceptor = TlsAcceptor::from(Arc::new(tls_config));

        info!("Listening on https://{} (TLS enabled)", addr);

        let listener = tokio::net::TcpListener::bind(addr).await?;

        // Run TLS server
        loop {
            tokio::select! {
                result = listener.accept() => {
                    let (stream, peer_addr) = result?;
                    let acceptor = tls_acceptor.clone();
                    let app = app.clone();

                    tokio::spawn(async move {
                        match acceptor.accept(stream).await {
                            Ok(tls_stream) => {
                                let io = hyper_util::rt::TokioIo::new(tls_stream);
                                let service = hyper::service::service_fn(move |req| {
                                    let mut app = app.clone();
                                    async move {
                                        app.call(req).await
                                    }
                                });

                                if let Err(e) = hyper_util::server::conn::auto::Builder::new(
                                    hyper_util::rt::TokioExecutor::new()
                                )
                                .serve_connection(io, service)
                                .await
                                {
                                    tracing::debug!("Error serving connection from {}: {}", peer_addr, e);
                                }
                            }
                            Err(e) => {
                                tracing::debug!("TLS handshake failed from {}: {}", peer_addr, e);
                            }
                        }
                    });
                }
                _ = shutdown_signal() => {
                    info!("Shutdown signal received");
                    break;
                }
            }
        }
    } else {
        info!("Listening on http://{}", addr);

        let listener = tokio::net::TcpListener::bind(addr).await?;
        axum::serve(listener, app)
            .with_graceful_shutdown(shutdown_signal())
            .await?;
    }

    info!("Server stopped");
    Ok(())
}

/// Initialize logging
fn init_logging(level: &str) {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level));

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
            metrics::describe_counter!("harbor_cache_hits_total", "Total number of cache hits");
            metrics::describe_counter!("harbor_cache_misses_total", "Total number of cache misses");
            metrics::describe_gauge!("harbor_cache_size_bytes", "Current cache size in bytes");
            metrics::describe_gauge!("harbor_cache_entries", "Current number of cache entries");
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
}

/// Load TLS configuration from certificate and key files
fn load_tls_config(tls_config: &config::TlsConfig) -> Result<RustlsServerConfig> {
    use tokio_rustls::rustls::crypto::aws_lc_rs;

    // Install the crypto provider
    let _ = aws_lc_rs::default_provider().install_default();

    let cert_path = tls_config
        .cert_path
        .as_ref()
        .context("TLS certificate path not configured")?;
    let key_path = tls_config
        .key_path
        .as_ref()
        .context("TLS key path not configured")?;

    // Load certificates
    let cert_file = File::open(cert_path)
        .with_context(|| format!("Failed to open certificate file: {}", cert_path))?;
    let mut cert_reader = BufReader::new(cert_file);
    let certs: Vec<CertificateDer<'static>> = rustls_pemfile::certs(&mut cert_reader)
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| format!("Failed to parse certificate file: {}", cert_path))?;

    if certs.is_empty() {
        anyhow::bail!("No certificates found in {}", cert_path);
    }

    // Load private key
    let key_file =
        File::open(key_path).with_context(|| format!("Failed to open key file: {}", key_path))?;
    let mut key_reader = BufReader::new(key_file);
    let key = load_private_key(&mut key_reader)
        .with_context(|| format!("Failed to parse key file: {}", key_path))?;

    // Build TLS config
    let config = RustlsServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .context("Failed to build TLS configuration")?;

    info!(
        "TLS configuration loaded from {} and {}",
        cert_path, key_path
    );
    Ok(config)
}

/// Load private key from PEM file (supports RSA, PKCS8, and EC keys)
fn load_private_key(reader: &mut BufReader<File>) -> Result<PrivateKeyDer<'static>> {
    use rustls_pemfile::Item;

    loop {
        match rustls_pemfile::read_one(reader)? {
            Some(Item::Pkcs1Key(key)) => return Ok(PrivateKeyDer::Pkcs1(key)),
            Some(Item::Pkcs8Key(key)) => return Ok(PrivateKeyDer::Pkcs8(key)),
            Some(Item::Sec1Key(key)) => return Ok(PrivateKeyDer::Sec1(key)),
            Some(_) => continue, // Skip other items
            None => anyhow::bail!("No private key found in key file"),
        }
    }
}
