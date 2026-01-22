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

// ==================== Input Validation ====================

/// Maximum allowed username length
const MAX_USERNAME_LENGTH: usize = 64;
/// Maximum allowed password length (prevent DoS with very large passwords)
const MAX_PASSWORD_LENGTH: usize = 256;

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

// ==================== Auth Routes ====================

/// POST /api/v1/auth/login
async fn login(
    State(state): State<AppState>,
    Json(request): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, ApiError> {
    // Validate input lengths to prevent DoS
    validate_username(&request.username)?;
    if request.password.len() > MAX_PASSWORD_LENGTH {
        return Err(ApiError::BadRequest(format!(
            "Password exceeds maximum length of {} characters",
            MAX_PASSWORD_LENGTH
        )));
    }

    debug!("Login attempt for user: {}", request.username);

    // Find user - but don't return early to prevent timing attacks
    let user_result = state
        .db
        .get_user_by_username(&request.username)
        .await?;

    // Verify password - always perform verification to prevent timing attacks
    // Use a dummy hash when user doesn't exist to maintain constant-time behavior
    // This dummy hash is a valid Argon2 hash that will always fail verification
    const DUMMY_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$dGltaW5nX2F0dGFja19wcmV2ZW50aW9u$K8rI5T7VdQ8xkO0GqK5K2w";

    let (hash_to_verify, user) = match user_result {
        Some(u) => (u.password_hash.clone(), Some(u)),
        None => (DUMMY_HASH.to_string(), None),
    };

    let password_valid = verify_password(&request.password, &hash_to_verify)?;

    // Return unauthorized if user doesn't exist or password is invalid
    let user = match (user, password_valid) {
        (Some(u), true) => u,
        _ => return Err(ApiError::Unauthorized),
    };

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
