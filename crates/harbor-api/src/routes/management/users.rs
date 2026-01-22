//! User management routes

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post, put},
};
use harbor_auth::hash_password;
use harbor_db::{NewUser, UserRole};
use tracing::{debug, info};

use crate::error::ApiError;
use crate::state::AppState;

use super::auth::RequireAdmin;
use super::types::{CreateUserRequest, UpdateUserRequest, UserResponse};

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

    let role: UserRole = request
        .role
        .parse()
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
        let role: UserRole = role_str
            .parse()
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

/// Create user management routes
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/users", get(list_users))
        .route("/api/v1/users", post(create_user))
        .route("/api/v1/users/{id}", get(get_user))
        .route("/api/v1/users/{id}", put(update_user))
        .route("/api/v1/users/{id}", delete(delete_user))
}
