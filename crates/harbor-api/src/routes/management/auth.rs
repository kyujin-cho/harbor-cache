//! Authentication extractors and routes

use axum::{
    extract::{FromRef, FromRequestParts, State},
    http::{header::AUTHORIZATION, request::Parts},
    routing::post,
    Json, Router,
};
use harbor_auth::{verify_password, AuthUser};
use harbor_db::UserRole;
use tracing::{debug, info};

use crate::error::ApiError;
use crate::state::AppState;

use super::types::{LoginRequest, LoginResponse};

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
pub struct RequireAdmin(pub AuthUser);

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

/// Create auth routes
pub fn routes() -> Router<AppState> {
    Router::new().route("/api/v1/auth/login", post(login))
}
