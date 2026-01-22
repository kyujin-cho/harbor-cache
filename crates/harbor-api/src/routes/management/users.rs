//! User management routes

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post, put},
    Json, Router,
};
use harbor_auth::hash_password;
use harbor_db::{NewUser, UserRole};
use tracing::{debug, info};

use crate::error::ApiError;
use crate::state::AppState;

use super::auth::RequireAdmin;
use super::types::{CreateUserRequest, UpdateUserRequest, UserResponse};

// ==================== Input Validation ====================

/// Maximum allowed username length
const MAX_USERNAME_LENGTH: usize = 64;
/// Maximum allowed password length
const MAX_PASSWORD_LENGTH: usize = 256;
/// Minimum allowed password length
const MIN_PASSWORD_LENGTH: usize = 8;

/// Validate username format and length
fn validate_username(username: &str) -> Result<(), ApiError> {
    if username.is_empty() {
        return Err(ApiError::BadRequest("Username cannot be empty".to_string()));
    }
    if username.len() > MAX_USERNAME_LENGTH {
        return Err(ApiError::BadRequest(format!(
            "Username exceeds maximum length of {} characters",
            MAX_USERNAME_LENGTH
        )));
    }
    // Only allow alphanumeric characters, underscores, and hyphens
    if !username.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') {
        return Err(ApiError::BadRequest(
            "Username can only contain alphanumeric characters, underscores, and hyphens".to_string(),
        ));
    }
    Ok(())
}

/// Validate password length
fn validate_password(password: &str) -> Result<(), ApiError> {
    if password.len() < MIN_PASSWORD_LENGTH {
        return Err(ApiError::BadRequest(format!(
            "Password must be at least {} characters long",
            MIN_PASSWORD_LENGTH
        )));
    }
    if password.len() > MAX_PASSWORD_LENGTH {
        return Err(ApiError::BadRequest(format!(
            "Password exceeds maximum length of {} characters",
            MAX_PASSWORD_LENGTH
        )));
    }
    Ok(())
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
    // Validate inputs
    validate_username(&request.username)?;
    validate_password(&request.password)?;

    debug!("Creating user: {}", request.username);

    let role = UserRole::from_str(&request.role)
        .ok_or_else(|| ApiError::BadRequest(format!("Invalid role: {}", request.role)))?;

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
        let role = UserRole::from_str(role_str)
            .ok_or_else(|| ApiError::BadRequest(format!("Invalid role: {}", role_str)))?;
        state.db.update_user_role(id, role).await?;
    }

    // Update password if provided
    if let Some(password) = &request.password {
        validate_password(password)?;
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

/// Create user routes
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/users", get(list_users))
        .route("/api/v1/users", post(create_user))
        .route("/api/v1/users/{id}", get(get_user))
        .route("/api/v1/users/{id}", put(update_user))
        .route("/api/v1/users/{id}", delete(delete_user))
}
