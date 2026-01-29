//! Cache management routes

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{delete, get, post},
};
use harbor_db::{repository::CacheEntryQuery, utils::format_bytes};
use tracing::{debug, info};

use crate::error::ApiError;
use crate::state::AppState;

use super::auth::{RequireAdmin, RequireAuth};
use super::types::{
    CacheEntriesListResponse, CacheEntriesQuery, CacheEntryResponse, CacheStatsResponse,
    CachedRepositoriesResponse,
};

// ==================== Cache Routes ====================

/// GET /api/v1/cache/stats (Authenticated)
async fn cache_stats(
    _auth: RequireAuth,
    State(state): State<AppState>,
) -> Result<Json<CacheStatsResponse>, ApiError> {
    let stats = state.cache.stats().await;

    let hit_rate = if stats.hit_count + stats.miss_count > 0 {
        stats.hit_count as f64 / (stats.hit_count + stats.miss_count) as f64
    } else {
        0.0
    };

    Ok(Json(CacheStatsResponse {
        total_size: stats.total_size,
        total_size_human: format_bytes(stats.total_size),
        entry_count: stats.entry_count,
        manifest_count: stats.manifest_count,
        blob_count: stats.blob_count,
        hit_count: stats.hit_count,
        miss_count: stats.miss_count,
        hit_rate,
    }))
}

/// GET /api/v1/cache/entries (Authenticated)
async fn list_cache_entries(
    _auth: RequireAuth,
    State(state): State<AppState>,
    Query(query): Query<CacheEntriesQuery>,
) -> Result<Json<CacheEntriesListResponse>, ApiError> {
    let db_query = CacheEntryQuery {
        entry_type: query.entry_type,
        repository: query.repository,
        digest: query.digest,
        offset: query.offset,
        limit: query.limit.min(100), // Cap at 100
        sort_by: query.sort_by,
        sort_order: query.sort_order,
    };

    let (entries, total) = state.db.list_cache_entries(db_query).await?;

    let response_entries: Vec<CacheEntryResponse> = entries
        .into_iter()
        .map(|e| CacheEntryResponse {
            id: e.id,
            entry_type: e.entry_type.as_str().to_string(),
            repository: e.repository,
            reference: e.reference,
            digest: e.digest,
            content_type: e.content_type,
            size: e.size,
            size_human: format_bytes(e.size),
            created_at: e.created_at.to_rfc3339(),
            last_accessed_at: e.last_accessed_at.to_rfc3339(),
            access_count: e.access_count,
        })
        .collect();

    Ok(Json(CacheEntriesListResponse {
        entries: response_entries,
        total,
        offset: query.offset,
        limit: query.limit,
    }))
}

/// GET /api/v1/cache/entries/top (Authenticated)
async fn top_accessed_entries(
    _auth: RequireAuth,
    State(state): State<AppState>,
) -> Result<Json<Vec<CacheEntryResponse>>, ApiError> {
    let entries = state.db.get_top_accessed_entries(10).await?;

    let response_entries: Vec<CacheEntryResponse> = entries
        .into_iter()
        .map(|e| CacheEntryResponse {
            id: e.id,
            entry_type: e.entry_type.as_str().to_string(),
            repository: e.repository,
            reference: e.reference,
            digest: e.digest,
            content_type: e.content_type,
            size: e.size,
            size_human: format_bytes(e.size),
            created_at: e.created_at.to_rfc3339(),
            last_accessed_at: e.last_accessed_at.to_rfc3339(),
            access_count: e.access_count,
        })
        .collect();

    Ok(Json(response_entries))
}

/// GET /api/v1/cache/repositories (Authenticated)
async fn cached_repositories(
    _auth: RequireAuth,
    State(state): State<AppState>,
) -> Result<Json<CachedRepositoriesResponse>, ApiError> {
    let repositories = state.db.get_cached_repositories().await?;

    Ok(Json(CachedRepositoriesResponse { repositories }))
}

/// DELETE /api/v1/cache/entries/:digest (Admin only)
async fn delete_cache_entry(
    _admin: RequireAdmin,
    State(state): State<AppState>,
    Path(digest): Path<String>,
) -> Result<StatusCode, ApiError> {
    debug!("Deleting cache entry: {}", digest);

    let deleted = state.cache.delete(&digest).await?;

    if deleted {
        info!("Deleted cache entry: {}", digest);
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound(format!("Cache entry: {}", digest)))
    }
}

/// DELETE /api/v1/cache (Admin only)
async fn clear_cache(
    _admin: RequireAdmin,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    info!("Clearing cache");

    let count = state.cache.clear().await?;

    Ok(Json(serde_json::json!({
        "cleared": count
    })))
}

/// POST /api/v1/cache/cleanup (Admin only)
async fn cleanup_cache(
    _admin: RequireAdmin,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    info!("Running cache cleanup");

    let count = state.cache.cleanup_expired().await?;

    Ok(Json(serde_json::json!({
        "cleaned": count
    })))
}

/// Create cache management routes
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/cache/stats", get(cache_stats))
        .route("/api/v1/cache/entries", get(list_cache_entries))
        .route("/api/v1/cache/entries/top", get(top_accessed_entries))
        .route("/api/v1/cache/repositories", get(cached_repositories))
        .route("/api/v1/cache/entries/{digest}", delete(delete_cache_entry))
        .route("/api/v1/cache", delete(clear_cache))
        .route("/api/v1/cache/cleanup", post(cleanup_cache))
}
