//! Upstream management routes

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post, put},
};
use harbor_db::{CacheIsolation, NewUpstream, NewUpstreamRoute, UpdateUpstream};
use harbor_proxy::{HarborClient, HarborClientConfig};
use std::net::IpAddr;
use tracing::{debug, info};
use url::Url;

use crate::error::ApiError;
use crate::state::AppState;

use super::auth::RequireAdmin;
use super::types::{
    CreateUpstreamRequest, TestUpstreamRequest, TestUpstreamResponse, UpdateUpstreamRequest,
    UpstreamHealthResponse, UpstreamResponse, UpstreamRouteResponse,
};

// ==================== Input Validation ====================

/// Maximum length for upstream name
const MAX_NAME_LENGTH: usize = 64;
/// Minimum length for upstream name
const MIN_NAME_LENGTH: usize = 1;
/// Maximum length for display name
const MAX_DISPLAY_NAME_LENGTH: usize = 128;
/// Maximum length for URL
const MAX_URL_LENGTH: usize = 2048;
/// Maximum length for registry name
const MAX_REGISTRY_LENGTH: usize = 256;
/// Maximum length for route pattern
const MAX_PATTERN_LENGTH: usize = 512;
/// Maximum number of wildcards in a pattern
const MAX_WILDCARDS_IN_PATTERN: usize = 10;

/// Validate upstream URL to prevent SSRF attacks.
/// Only allows HTTP/HTTPS URLs to external hosts.
fn validate_upstream_url(url_str: &str) -> Result<(), ApiError> {
    // Check length first
    if url_str.len() > MAX_URL_LENGTH {
        return Err(ApiError::BadRequest(format!(
            "URL exceeds maximum length of {} characters",
            MAX_URL_LENGTH
        )));
    }

    // Parse the URL
    let url = Url::parse(url_str)
        .map_err(|e| ApiError::BadRequest(format!("Invalid URL format: {}", e)))?;

    // Only allow HTTP and HTTPS schemes
    match url.scheme() {
        "http" | "https" => {}
        scheme => {
            return Err(ApiError::BadRequest(format!(
                "URL scheme '{}' is not allowed. Only http and https are permitted",
                scheme
            )));
        }
    }

    // Get the host
    let host = url
        .host_str()
        .ok_or_else(|| ApiError::BadRequest("URL must have a host".to_string()))?;

    // Block localhost and loopback addresses
    if host == "localhost" || host == "127.0.0.1" || host == "::1" {
        return Err(ApiError::BadRequest(
            "Localhost URLs are not allowed for security reasons".to_string(),
        ));
    }

    // Block common internal hostnames
    let lower_host = host.to_lowercase();
    if lower_host == "metadata"
        || lower_host == "metadata.google.internal"
        || lower_host.ends_with(".internal")
        || lower_host.ends_with(".local")
    {
        return Err(ApiError::BadRequest(
            "Internal hostnames are not allowed for security reasons".to_string(),
        ));
    }

    // Try to parse as IP address and block private/internal ranges
    if let Ok(ip) = host.parse::<IpAddr>()
        && is_private_or_reserved_ip(&ip)
    {
        return Err(ApiError::BadRequest(
            "Private or reserved IP addresses are not allowed for security reasons".to_string(),
        ));
    }

    Ok(())
}

/// Check if an IP address is private, loopback, or otherwise reserved
fn is_private_or_reserved_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(ipv4) => {
            ipv4.is_loopback()                    // 127.0.0.0/8
                || ipv4.is_private()              // 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16
                || ipv4.is_link_local()           // 169.254.0.0/16
                || ipv4.is_broadcast()            // 255.255.255.255
                || ipv4.is_unspecified()          // 0.0.0.0
                || ipv4.octets()[0] == 169        // Cloud metadata (169.254.169.254)
                || ipv4.is_documentation() // 192.0.2.0/24, 198.51.100.0/24, 203.0.113.0/24
        }
        IpAddr::V6(ipv6) => {
            ipv6.is_loopback()                    // ::1
                || ipv6.is_unspecified()          // ::
                // IPv4-mapped IPv6 addresses
                || (ipv6.segments()[0..6] == [0, 0, 0, 0, 0, 0xFFFF]
                    && is_private_or_reserved_ip(&IpAddr::V4(std::net::Ipv4Addr::new(
                        (ipv6.segments()[6] >> 8) as u8,
                        (ipv6.segments()[6] & 0xFF) as u8,
                        (ipv6.segments()[7] >> 8) as u8,
                        (ipv6.segments()[7] & 0xFF) as u8,
                    ))))
        }
    }
}

/// Validate upstream name format and length
fn validate_upstream_name(name: &str) -> Result<(), ApiError> {
    if name.len() < MIN_NAME_LENGTH {
        return Err(ApiError::BadRequest(
            "Upstream name cannot be empty".to_string(),
        ));
    }

    if name.len() > MAX_NAME_LENGTH {
        return Err(ApiError::BadRequest(format!(
            "Upstream name exceeds maximum length of {} characters",
            MAX_NAME_LENGTH
        )));
    }

    // Must contain only alphanumeric, dashes, and underscores
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(ApiError::BadRequest(
            "Upstream name must contain only alphanumeric characters, dashes, and underscores"
                .to_string(),
        ));
    }

    // Must start with alphanumeric
    if let Some(first) = name.chars().next()
        && !first.is_ascii_alphanumeric()
    {
        return Err(ApiError::BadRequest(
            "Upstream name must start with an alphanumeric character".to_string(),
        ));
    }

    Ok(())
}

/// Validate display name format and length
fn validate_display_name(name: &str) -> Result<(), ApiError> {
    if name.is_empty() {
        return Err(ApiError::BadRequest(
            "Display name cannot be empty".to_string(),
        ));
    }

    if name.len() > MAX_DISPLAY_NAME_LENGTH {
        return Err(ApiError::BadRequest(format!(
            "Display name exceeds maximum length of {} characters",
            MAX_DISPLAY_NAME_LENGTH
        )));
    }

    // Block obvious script injection attempts
    let lower = name.to_lowercase();
    if lower.contains("<script") || lower.contains("javascript:") || lower.contains("data:") {
        return Err(ApiError::BadRequest(
            "Display name contains disallowed characters".to_string(),
        ));
    }

    Ok(())
}

/// Validate registry name
fn validate_registry_name(registry: &str) -> Result<(), ApiError> {
    if registry.is_empty() {
        return Err(ApiError::BadRequest(
            "Registry name cannot be empty".to_string(),
        ));
    }

    if registry.len() > MAX_REGISTRY_LENGTH {
        return Err(ApiError::BadRequest(format!(
            "Registry name exceeds maximum length of {} characters",
            MAX_REGISTRY_LENGTH
        )));
    }

    Ok(())
}

/// Validate route pattern to prevent ReDoS and other issues
fn validate_route_pattern(pattern: &str) -> Result<(), ApiError> {
    if pattern.is_empty() {
        return Err(ApiError::BadRequest(
            "Route pattern cannot be empty".to_string(),
        ));
    }

    if pattern.len() > MAX_PATTERN_LENGTH {
        return Err(ApiError::BadRequest(format!(
            "Route pattern exceeds maximum length of {} characters",
            MAX_PATTERN_LENGTH
        )));
    }

    // Count wildcards to prevent ReDoS
    let wildcard_count = pattern.matches('*').count();
    if wildcard_count > MAX_WILDCARDS_IN_PATTERN {
        return Err(ApiError::BadRequest(format!(
            "Route pattern contains too many wildcards (max {})",
            MAX_WILDCARDS_IN_PATTERN
        )));
    }

    // Block path traversal attempts
    if pattern.contains("..") {
        return Err(ApiError::BadRequest(
            "Route pattern cannot contain path traversal sequences".to_string(),
        ));
    }

    Ok(())
}

// ==================== Helper Functions ====================

fn upstream_to_response(upstream: harbor_db::Upstream) -> UpstreamResponse {
    UpstreamResponse {
        id: upstream.id,
        name: upstream.name,
        display_name: upstream.display_name,
        url: upstream.url,
        registry: upstream.registry,
        skip_tls_verify: upstream.skip_tls_verify,
        priority: upstream.priority,
        enabled: upstream.enabled,
        cache_isolation: upstream.cache_isolation.as_str().to_string(),
        is_default: upstream.is_default,
        has_credentials: upstream.username.is_some(),
        created_at: upstream.created_at.to_rfc3339(),
        updated_at: upstream.updated_at.to_rfc3339(),
    }
}

fn route_to_response(route: harbor_db::UpstreamRoute) -> UpstreamRouteResponse {
    UpstreamRouteResponse {
        id: route.id,
        upstream_id: route.upstream_id,
        pattern: route.pattern,
        priority: route.priority,
        created_at: route.created_at.to_rfc3339(),
    }
}

// ==================== Upstream Routes ====================

/// GET /api/v1/upstreams (Admin only)
async fn list_upstreams(
    _admin: RequireAdmin,
    State(state): State<AppState>,
) -> Result<Json<Vec<UpstreamResponse>>, ApiError> {
    let upstreams = state.db.list_upstreams().await?;

    Ok(Json(
        upstreams.into_iter().map(upstream_to_response).collect(),
    ))
}

/// POST /api/v1/upstreams (Admin only)
async fn create_upstream(
    _admin: RequireAdmin,
    State(state): State<AppState>,
    Json(request): Json<CreateUpstreamRequest>,
) -> Result<(StatusCode, Json<UpstreamResponse>), ApiError> {
    debug!("Creating upstream: {}", request.name);

    // Validate all input fields
    validate_upstream_name(&request.name)?;
    validate_display_name(&request.display_name)?;
    validate_upstream_url(&request.url)?;
    validate_registry_name(&request.registry)?;

    // Validate routes if provided
    for route in &request.routes {
        validate_route_pattern(&route.pattern)?;
    }

    // Check for duplicate name
    if state
        .db
        .get_upstream_by_name(&request.name)
        .await?
        .is_some()
    {
        return Err(ApiError::BadRequest(format!(
            "Upstream with name '{}' already exists",
            request.name
        )));
    }

    // Parse cache isolation
    let cache_isolation: CacheIsolation = request
        .cache_isolation
        .parse()
        .unwrap_or(CacheIsolation::Shared);

    // Create the upstream
    let upstream = state
        .db
        .insert_upstream(NewUpstream {
            name: request.name.clone(),
            display_name: request.display_name,
            url: request.url,
            registry: request.registry,
            username: request.username,
            password: request.password,
            skip_tls_verify: request.skip_tls_verify,
            priority: request.priority,
            enabled: request.enabled,
            cache_isolation,
            is_default: request.is_default,
        })
        .await?;

    // Create routes if provided
    for route in request.routes {
        state
            .db
            .insert_upstream_route(NewUpstreamRoute {
                upstream_id: upstream.id,
                pattern: route.pattern,
                priority: route.priority,
            })
            .await?;
    }

    info!("Created upstream: {} (id: {})", request.name, upstream.id);

    Ok((StatusCode::CREATED, Json(upstream_to_response(upstream))))
}

/// GET /api/v1/upstreams/:id (Admin only)
async fn get_upstream(
    _admin: RequireAdmin,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<UpstreamResponse>, ApiError> {
    let upstream = state
        .db
        .get_upstream(id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Upstream: {}", id)))?;

    Ok(Json(upstream_to_response(upstream)))
}

/// PUT /api/v1/upstreams/:id (Admin only)
async fn update_upstream(
    _admin: RequireAdmin,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(request): Json<UpdateUpstreamRequest>,
) -> Result<Json<UpstreamResponse>, ApiError> {
    debug!("Updating upstream: {}", id);

    // Validate updated fields if provided
    if let Some(ref display_name) = request.display_name {
        validate_display_name(display_name)?;
    }
    if let Some(ref url) = request.url {
        validate_upstream_url(url)?;
    }
    if let Some(ref registry) = request.registry {
        validate_registry_name(registry)?;
    }

    // Verify upstream exists
    let _upstream = state
        .db
        .get_upstream(id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Upstream: {}", id)))?;

    // Parse cache isolation if provided
    let cache_isolation = request
        .cache_isolation
        .as_ref()
        .map(|s| s.parse().unwrap_or(CacheIsolation::Shared));

    // Build update
    let update = UpdateUpstream {
        display_name: request.display_name,
        url: request.url,
        registry: request.registry,
        username: request.username.map(Some),
        password: request.password.map(Some),
        skip_tls_verify: request.skip_tls_verify,
        priority: request.priority,
        enabled: request.enabled,
        cache_isolation,
        is_default: request.is_default,
    };

    let upstream = state
        .db
        .update_upstream(id, update)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Upstream: {}", id)))?;

    info!("Updated upstream: {} (id: {})", upstream.name, id);

    Ok(Json(upstream_to_response(upstream)))
}

/// DELETE /api/v1/upstreams/:id (Admin only)
async fn delete_upstream(
    _admin: RequireAdmin,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    debug!("Deleting upstream: {}", id);

    let deleted = state.db.delete_upstream(id).await?;

    if deleted {
        info!("Deleted upstream: {}", id);
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound(format!("Upstream: {}", id)))
    }
}

// ==================== Route Management ====================

/// GET /api/v1/upstreams/:id/routes (Admin only)
async fn list_upstream_routes(
    _admin: RequireAdmin,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Vec<UpstreamRouteResponse>>, ApiError> {
    // Verify upstream exists
    let _upstream = state
        .db
        .get_upstream(id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Upstream: {}", id)))?;

    let routes = state.db.get_upstream_routes(id).await?;

    Ok(Json(routes.into_iter().map(route_to_response).collect()))
}

/// POST /api/v1/upstreams/:id/routes (Admin only)
async fn add_upstream_route(
    _admin: RequireAdmin,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(request): Json<super::types::CreateRouteRequest>,
) -> Result<(StatusCode, Json<UpstreamRouteResponse>), ApiError> {
    debug!("Adding route to upstream {}: {}", id, request.pattern);

    // Validate route pattern
    validate_route_pattern(&request.pattern)?;

    // Verify upstream exists
    let _upstream = state
        .db
        .get_upstream(id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Upstream: {}", id)))?;

    let route = state
        .db
        .insert_upstream_route(NewUpstreamRoute {
            upstream_id: id,
            pattern: request.pattern.clone(),
            priority: request.priority,
        })
        .await?;

    info!("Added route {} to upstream {}", request.pattern, id);

    Ok((StatusCode::CREATED, Json(route_to_response(route))))
}

/// DELETE /api/v1/upstreams/:upstream_id/routes/:route_id (Admin only)
async fn delete_upstream_route(
    _admin: RequireAdmin,
    State(state): State<AppState>,
    Path((upstream_id, route_id)): Path<(i64, i64)>,
) -> Result<StatusCode, ApiError> {
    debug!("Deleting route {} from upstream {}", route_id, upstream_id);

    // Verify upstream exists
    let _upstream = state
        .db
        .get_upstream(upstream_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Upstream: {}", upstream_id)))?;

    let deleted = state.db.delete_upstream_route(route_id).await?;

    if deleted {
        info!("Deleted route {} from upstream {}", route_id, upstream_id);
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound(format!("Route: {}", route_id)))
    }
}

// ==================== Health & Testing ====================

/// GET /api/v1/upstreams/:id/health (Admin only)
async fn get_upstream_health(
    _admin: RequireAdmin,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<UpstreamHealthResponse>, ApiError> {
    let upstream = state
        .db
        .get_upstream(id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Upstream: {}", id)))?;

    // Test connection
    let config = HarborClientConfig {
        url: upstream.url.clone(),
        registry: upstream.registry.clone(),
        username: upstream.username.clone(),
        password: upstream.password.clone(),
        skip_tls_verify: upstream.skip_tls_verify,
    };

    let (healthy, error) = match HarborClient::new(config) {
        Ok(client) => match client.ping().await {
            Ok(true) => (true, None),
            Ok(false) => (false, Some("Ping returned false".to_string())),
            Err(e) => (false, Some(e.to_string())),
        },
        Err(e) => (false, Some(e.to_string())),
    };

    Ok(Json(UpstreamHealthResponse {
        upstream_id: id,
        name: upstream.name,
        healthy,
        last_check: chrono::Utc::now().to_rfc3339(),
        last_error: error,
        consecutive_failures: if healthy { 0 } else { 1 },
    }))
}

/// GET /api/v1/upstreams/health (Admin only) - Get health for all upstreams
async fn get_all_upstreams_health(
    _admin: RequireAdmin,
    State(state): State<AppState>,
) -> Result<Json<Vec<UpstreamHealthResponse>>, ApiError> {
    let upstreams = state.db.list_upstreams().await?;

    // Run health checks concurrently for better performance
    let health_futures: Vec<_> = upstreams
        .into_iter()
        .map(|upstream| {
            let config = HarborClientConfig {
                url: upstream.url.clone(),
                registry: upstream.registry.clone(),
                username: upstream.username.clone(),
                password: upstream.password.clone(),
                skip_tls_verify: upstream.skip_tls_verify,
            };

            async move {
                let (healthy, error) = match HarborClient::new(config) {
                    Ok(client) => {
                        // Add timeout for individual health check (10 seconds)
                        match tokio::time::timeout(
                            std::time::Duration::from_secs(10),
                            client.ping(),
                        )
                        .await
                        {
                            Ok(Ok(true)) => (true, None),
                            Ok(Ok(false)) => (false, Some("Ping returned false".to_string())),
                            Ok(Err(e)) => (false, Some(e.to_string())),
                            Err(_) => (false, Some("Health check timed out".to_string())),
                        }
                    }
                    Err(e) => (false, Some(e.to_string())),
                };

                UpstreamHealthResponse {
                    upstream_id: upstream.id,
                    name: upstream.name,
                    healthy,
                    last_check: chrono::Utc::now().to_rfc3339(),
                    last_error: error,
                    consecutive_failures: if healthy { 0 } else { 1 },
                }
            }
        })
        .collect();

    let results = futures::future::join_all(health_futures).await;

    Ok(Json(results))
}

/// POST /api/v1/upstreams/test (Admin only) - Test connection without saving
async fn test_upstream_connection(
    _admin: RequireAdmin,
    Json(request): Json<TestUpstreamRequest>,
) -> Result<Json<TestUpstreamResponse>, ApiError> {
    debug!("Testing upstream connection: {}", request.url);

    // Validate URL to prevent SSRF attacks
    validate_upstream_url(&request.url)?;
    validate_registry_name(&request.registry)?;

    let config = HarborClientConfig {
        url: request.url.clone(),
        registry: request.registry,
        username: request.username,
        password: request.password,
        skip_tls_verify: request.skip_tls_verify,
    };

    match HarborClient::new(config) {
        Ok(client) => match client.ping().await {
            Ok(true) => Ok(Json(TestUpstreamResponse {
                success: true,
                message: "Connection successful".to_string(),
            })),
            Ok(false) => Ok(Json(TestUpstreamResponse {
                success: false,
                message: "Ping returned false".to_string(),
            })),
            Err(e) => Ok(Json(TestUpstreamResponse {
                success: false,
                message: format!("Connection failed: {}", e),
            })),
        },
        Err(e) => Ok(Json(TestUpstreamResponse {
            success: false,
            message: format!("Failed to create client: {}", e),
        })),
    }
}

/// GET /api/v1/upstreams/:id/stats (Admin only) - Get cache statistics for an upstream
async fn get_upstream_stats(
    _admin: RequireAdmin,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<super::types::CacheStatsResponse>, ApiError> {
    // Verify upstream exists
    let _upstream = state
        .db
        .get_upstream(id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Upstream: {}", id)))?;

    let stats = state.db.get_cache_stats_by_upstream(id).await?;

    Ok(Json(super::types::CacheStatsResponse {
        total_size: stats.total_size,
        total_size_human: format_size(stats.total_size as u64),
        entry_count: stats.entry_count,
        manifest_count: stats.manifest_count,
        blob_count: stats.blob_count,
        hit_count: stats.hit_count,
        miss_count: stats.miss_count,
        hit_rate: if stats.hit_count + stats.miss_count > 0 {
            stats.hit_count as f64 / (stats.hit_count + stats.miss_count) as f64
        } else {
            0.0
        },
    }))
}

/// Format bytes as human-readable size
fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];
    let mut size = bytes as f64;
    let mut unit_index = 0;

    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }

    if unit_index == 0 {
        format!("{} {}", bytes, UNITS[0])
    } else {
        format!("{:.2} {}", size, UNITS[unit_index])
    }
}

/// Create upstream management routes
pub fn routes() -> Router<AppState> {
    Router::new()
        // Upstream CRUD
        .route("/api/v1/upstreams", get(list_upstreams))
        .route("/api/v1/upstreams", post(create_upstream))
        .route("/api/v1/upstreams/health", get(get_all_upstreams_health))
        .route("/api/v1/upstreams/test", post(test_upstream_connection))
        .route("/api/v1/upstreams/{id}", get(get_upstream))
        .route("/api/v1/upstreams/{id}", put(update_upstream))
        .route("/api/v1/upstreams/{id}", delete(delete_upstream))
        // Routes management
        .route("/api/v1/upstreams/{id}/routes", get(list_upstream_routes))
        .route("/api/v1/upstreams/{id}/routes", post(add_upstream_route))
        .route(
            "/api/v1/upstreams/{upstream_id}/routes/{route_id}",
            delete(delete_upstream_route),
        )
        // Health & Stats
        .route("/api/v1/upstreams/{id}/health", get(get_upstream_health))
        .route("/api/v1/upstreams/{id}/stats", get(get_upstream_stats))
}
