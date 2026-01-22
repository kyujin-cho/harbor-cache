//! Management API routes

use axum::{
    extract::{FromRef, FromRequestParts, Path, State},
    http::{header::AUTHORIZATION, request::Parts, StatusCode},
    routing::{delete, get, post, put},
    Json, Router,
};
use harbor_auth::{hash_password, verify_password, AuthUser};
use harbor_db::{NewUser, UserRole};
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use crate::error::ApiError;
use crate::state::AppState;

// ==================== Auth Extractors ====================

/// Extractor for authenticated user (required)
pub struct RequireAuth(pub AuthUser);

impl<S> FromRequestParts<S> for RequireAuth
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let app_state = AppState::from_ref(state);

        // Skip auth check if disabled
        if !app_state.auth_enabled {
            return Ok(RequireAuth(AuthUser {
                id: 0,
                username: "anonymous".to_string(),
                role: UserRole::Admin,
            }));
        }

        let auth_header = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|h| h.to_str().ok())
            .ok_or(ApiError::Unauthorized)?;

        if !auth_header.starts_with("Bearer ") {
            return Err(ApiError::Unauthorized);
        }

        let token = &auth_header[7..];
        let claims = app_state.jwt.validate_token(token).map_err(|_| ApiError::Unauthorized)?;
        let user = AuthUser::from_claims(&claims);

        debug!("Authenticated user: {} ({})", user.username, user.role.as_str());
        Ok(RequireAuth(user))
    }
}

/// Extractor for admin user (required)
#[allow(dead_code)]
pub struct RequireAdmin(AuthUser);

#[allow(dead_code)]
impl RequireAdmin {
    /// Get the authenticated admin user
    pub fn user(&self) -> &AuthUser {
        &self.0
    }
}

impl<S> FromRequestParts<S> for RequireAdmin
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let RequireAuth(user) = RequireAuth::from_request_parts(parts, state).await?;

        if !user.role.is_admin() {
            return Err(ApiError::Forbidden);
        }

        Ok(RequireAdmin(user))
    }
}

// ==================== Types ====================

/// Login request
#[derive(Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

/// Login response
#[derive(Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub expires_in: i64,
}

/// Create user request
#[derive(Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub password: String,
    pub role: String,
}

/// Update user request
#[derive(Deserialize)]
pub struct UpdateUserRequest {
    pub role: Option<String>,
    pub password: Option<String>,
}

/// User response (without password)
#[derive(Serialize)]
pub struct UserResponse {
    pub id: i64,
    pub username: String,
    pub role: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Cache statistics response
#[derive(Serialize)]
pub struct CacheStatsResponse {
    pub total_size: i64,
    pub total_size_human: String,
    pub entry_count: i64,
    pub manifest_count: i64,
    pub blob_count: i64,
    pub hit_count: i64,
    pub miss_count: i64,
    pub hit_rate: f64,
}

/// Config entry response
#[derive(Serialize)]
pub struct ConfigEntryResponse {
    pub key: String,
    pub value: String,
    pub updated_at: String,
}

/// Update config request
#[derive(Deserialize)]
pub struct UpdateConfigRequest {
    pub entries: Vec<ConfigUpdateEntry>,
}

/// Single config update entry
#[derive(Deserialize)]
pub struct ConfigUpdateEntry {
    pub key: String,
    pub value: String,
}

// ==================== Auth Routes ====================

/// POST /api/v1/auth/login
async fn login(
    State(state): State<AppState>,
    Json(request): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, ApiError> {
    debug!("Login attempt for user: {}", request.username);

    // Find user
    let user = state
        .db
        .get_user_by_username(&request.username)
        .await?
        .ok_or(ApiError::Unauthorized)?;

    // Verify password
    if !verify_password(&request.password, &user.password_hash)? {
        return Err(ApiError::Unauthorized);
    }

    // Generate token
    let token = state.jwt.generate_token(user.id, &user.username, user.role.as_str())?;

    info!("User {} logged in successfully", user.username);

    Ok(Json(LoginResponse {
        token,
        expires_in: 24 * 3600, // 24 hours
    }))
}

// ==================== User Routes ====================

/// GET /api/v1/users (Admin only)
async fn list_users(
    _admin: RequireAdmin,
    State(state): State<AppState>,
) -> Result<Json<Vec<UserResponse>>, ApiError> {
    let users = state.db.list_users().await?;

    Ok(Json(
        users
            .into_iter()
            .map(|u| UserResponse {
                id: u.id,
                username: u.username,
                role: u.role.as_str().to_string(),
                created_at: u.created_at.to_rfc3339(),
                updated_at: u.updated_at.to_rfc3339(),
            })
            .collect(),
    ))
}

/// POST /api/v1/users (Admin only)
async fn create_user(
    _admin: RequireAdmin,
    State(state): State<AppState>,
    Json(request): Json<CreateUserRequest>,
) -> Result<(StatusCode, Json<UserResponse>), ApiError> {
    debug!("Creating user: {}", request.username);

    let role: UserRole = request.role.parse()
        .map_err(|_| ApiError::BadRequest(format!("Invalid role: {}", request.role)))?;

    let password_hash = hash_password(&request.password)?;

    let user = state
        .db
        .insert_user(NewUser {
            username: request.username.clone(),
            password_hash,
            role,
        })
        .await?;

    info!("Created user: {}", user.username);

    Ok((
        StatusCode::CREATED,
        Json(UserResponse {
            id: user.id,
            username: user.username,
            role: user.role.as_str().to_string(),
            created_at: user.created_at.to_rfc3339(),
            updated_at: user.updated_at.to_rfc3339(),
        }),
    ))
}

/// GET /api/v1/users/:id (Admin only)
async fn get_user(
    _admin: RequireAdmin,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<UserResponse>, ApiError> {
    let user = state
        .db
        .get_user_by_id(id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("User: {}", id)))?;

    Ok(Json(UserResponse {
        id: user.id,
        username: user.username,
        role: user.role.as_str().to_string(),
        created_at: user.created_at.to_rfc3339(),
        updated_at: user.updated_at.to_rfc3339(),
    }))
}

/// PUT /api/v1/users/:id (Admin only)
async fn update_user(
    _admin: RequireAdmin,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(request): Json<UpdateUserRequest>,
) -> Result<Json<UserResponse>, ApiError> {
    debug!("Updating user: {}", id);

    // Verify user exists
    let _user = state
        .db
        .get_user_by_id(id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("User: {}", id)))?;

    // Update role if provided
    if let Some(role_str) = &request.role {
        let role: UserRole = role_str.parse()
            .map_err(|_| ApiError::BadRequest(format!("Invalid role: {}", role_str)))?;
        state.db.update_user_role(id, role).await?;
    }

    // Update password if provided
    if let Some(password) = &request.password {
        let password_hash = hash_password(password)?;
        state.db.update_user_password(id, &password_hash).await?;
    }

    // Fetch updated user
    let user = state
        .db
        .get_user_by_id(id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("User: {}", id)))?;

    info!("Updated user: {}", user.username);

    Ok(Json(UserResponse {
        id: user.id,
        username: user.username,
        role: user.role.as_str().to_string(),
        created_at: user.created_at.to_rfc3339(),
        updated_at: user.updated_at.to_rfc3339(),
    }))
}

/// DELETE /api/v1/users/:id (Admin only)
async fn delete_user(
    _admin: RequireAdmin,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    debug!("Deleting user: {}", id);

    let deleted = state.db.delete_user(id).await?;

    if deleted {
        info!("Deleted user: {}", id);
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound(format!("User: {}", id)))
    }
}

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

// ==================== Helper Functions ====================

/// Format bytes as human-readable string
fn format_bytes(bytes: i64) -> String {
    if bytes < 0 {
        return format!("{} B", bytes);
    }

    const KB: i64 = 1024;
    const MB: i64 = KB * 1024;
    const GB: i64 = MB * 1024;
    const TB: i64 = GB * 1024;

    if bytes >= TB {
        format!("{:.2} TB", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}


// ==================== Routes ====================

/// Create management API routes
pub fn routes() -> Router<AppState> {
    Router::new()
        // Auth (public)
        .route("/api/v1/auth/login", post(login))
        // Users (admin only via extractor)
        .route("/api/v1/users", get(list_users))
        .route("/api/v1/users", post(create_user))
        .route("/api/v1/users/{id}", get(get_user))
        .route("/api/v1/users/{id}", put(update_user))
        .route("/api/v1/users/{id}", delete(delete_user))
        // Cache (auth required via extractor)
        .route("/api/v1/cache/stats", get(cache_stats))
        .route("/api/v1/cache", delete(clear_cache))
        .route("/api/v1/cache/cleanup", post(cleanup_cache))
        // Config (admin only via extractor)
        .route("/api/v1/config", get(get_config))
        .route("/api/v1/config", put(update_config))
        .route("/api/v1/config/{key}", get(get_config_key))
        .route("/api/v1/config/{key}", delete(delete_config_key))
}
