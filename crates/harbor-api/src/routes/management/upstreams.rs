//! Upstream management routes

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post, put},
};
use harbor_db::{CacheIsolation, NewUpstream, NewUpstreamRoute, UpdateUpstream};
use harbor_proxy::{HarborClient, HarborClientConfig};
use tracing::{debug, info};

use crate::error::ApiError;
use crate::state::AppState;

use super::auth::RequireAdmin;
use super::types::{
    CreateUpstreamRequest, TestUpstreamRequest, TestUpstreamResponse,
    UpdateUpstreamRequest, UpstreamHealthResponse, UpstreamResponse,
    UpstreamRouteResponse,
};

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
        upstreams
            .into_iter()
            .map(upstream_to_response)
            .collect(),
    ))
}

/// POST /api/v1/upstreams (Admin only)
async fn create_upstream(
    _admin: RequireAdmin,
    State(state): State<AppState>,
    Json(request): Json<CreateUpstreamRequest>,
) -> Result<(StatusCode, Json<UpstreamResponse>), ApiError> {
    debug!("Creating upstream: {}", request.name);

    // Validate name (alphanumeric and dashes only)
    if !request.name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err(ApiError::BadRequest(
            "Upstream name must contain only alphanumeric characters, dashes, and underscores".to_string()
        ));
    }

    // Check for duplicate name
    if state.db.get_upstream_by_name(&request.name).await?.is_some() {
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
    let mut results = Vec::new();

    for upstream in upstreams {
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

        results.push(UpstreamHealthResponse {
            upstream_id: upstream.id,
            name: upstream.name,
            healthy,
            last_check: chrono::Utc::now().to_rfc3339(),
            last_error: error,
            consecutive_failures: if healthy { 0 } else { 1 },
        });
    }

    Ok(Json(results))
}

/// POST /api/v1/upstreams/test (Admin only) - Test connection without saving
async fn test_upstream_connection(
    _admin: RequireAdmin,
    Json(request): Json<TestUpstreamRequest>,
) -> Result<Json<TestUpstreamResponse>, ApiError> {
    debug!("Testing upstream connection: {}", request.url);

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
