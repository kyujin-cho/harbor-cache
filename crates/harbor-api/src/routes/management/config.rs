//! Configuration management routes

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, put},
    Json, Router,
};
use tracing::{debug, info};

use crate::error::ApiError;
use crate::state::AppState;

use super::auth::RequireAdmin;
use super::types::{ConfigEntryResponse, UpdateConfigRequest};

// ==================== Config Routes ====================

/// GET /api/v1/config (Admin only)
async fn get_config(
    _admin: RequireAdmin,
    State(state): State<AppState>,
) -> Result<Json<Vec<ConfigEntryResponse>>, ApiError> {
    let entries = state.db.list_config().await?;

    Ok(Json(
        entries
            .into_iter()
            .map(|e| ConfigEntryResponse {
                key: e.key,
                value: e.value,
                updated_at: e.updated_at.to_rfc3339(),
            })
            .collect(),
    ))
}

/// PUT /api/v1/config (Admin only)
async fn update_config(
    _admin: RequireAdmin,
    State(state): State<AppState>,
    Json(request): Json<UpdateConfigRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    info!("Updating {} config entries", request.entries.len());

    for entry in &request.entries {
        state.db.set_config(&entry.key, &entry.value).await?;
    }

    Ok(Json(serde_json::json!({
        "updated": request.entries.len()
    })))
}

/// GET /api/v1/config/:key (Admin only)
async fn get_config_key(
    _admin: RequireAdmin,
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<ConfigEntryResponse>, ApiError> {
    let entries = state.db.list_config().await?;

    let entry = entries
        .into_iter()
        .find(|e| e.key == key)
        .ok_or_else(|| ApiError::NotFound(format!("Config key: {}", key)))?;

    Ok(Json(ConfigEntryResponse {
        key: entry.key,
        value: entry.value,
        updated_at: entry.updated_at.to_rfc3339(),
    }))
}

/// DELETE /api/v1/config/:key (Admin only)
async fn delete_config_key(
    _admin: RequireAdmin,
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<StatusCode, ApiError> {
    debug!("Deleting config key: {}", key);

    let deleted = state.db.delete_config(&key).await?;

    if deleted {
        info!("Deleted config key: {}", key);
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound(format!("Config key: {}", key)))
    }
}

/// Create config routes
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/config", get(get_config))
        .route("/api/v1/config", put(update_config))
        .route("/api/v1/config/{key}", get(get_config_key))
        .route("/api/v1/config/{key}", delete(delete_config_key))
}
