//! Cache management routes

use axum::{
    Json, Router,
    extract::State,
    routing::{delete, get, post},
};
use harbor_db::utils::format_bytes;
use tracing::info;

use crate::error::ApiError;
use crate::state::AppState;

use super::auth::{RequireAdmin, RequireAuth};
use super::types::CacheStatsResponse;

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
        .route("/api/v1/cache", delete(clear_cache))
        .route("/api/v1/cache/cleanup", post(cleanup_cache))
}
