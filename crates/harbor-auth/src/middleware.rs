//! Authentication middleware for Axum

use axum::{
    extract::{Request, State},
    http::header::AUTHORIZATION,
    middleware::Next,
    response::Response,
};
use harbor_db::UserRole;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::debug;

use crate::error::AuthError;
use crate::jwt::{Claims, JwtManager};

/// Authenticated user information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthUser {
    pub id: i64,
    pub username: String,
    pub role: UserRole,
}

impl AuthUser {
    /// Create from JWT claims
    pub fn from_claims(claims: &Claims) -> Self {
        Self {
            id: claims.sub.parse().unwrap_or(0),
            username: claims.username.clone(),
            role: claims.role.parse().unwrap_or(UserRole::ReadOnly),
        }
    }
}

/// Extract bearer token from authorization header
fn extract_bearer_token(header: &str) -> Result<&str, AuthError> {
    if !header.starts_with("Bearer ") {
        return Err(AuthError::InvalidAuthHeader);
    }
    Ok(&header[7..])
}

/// Authentication middleware
///
/// This middleware extracts and validates JWT tokens from the Authorization header.
/// If valid, it adds the AuthUser to request extensions.
pub async fn auth_middleware(
    State(jwt_manager): State<Arc<JwtManager>>,
    mut request: Request,
    next: Next,
) -> Result<Response, AuthError> {
    // Extract Authorization header
    let auth_header = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|h| h.to_str().ok());

    if let Some(header) = auth_header {
        let token = extract_bearer_token(header)?;
        let claims = jwt_manager.validate_token(token)?;
        let user = AuthUser::from_claims(&claims);

        debug!(
            "Authenticated user: {} ({})",
            user.username,
            user.role.as_str()
        );

        // Add user to request extensions
        request.extensions_mut().insert(user);
    }

    Ok(next.run(request).await)
}

/// Middleware to require admin role
pub async fn require_admin(request: Request, next: Next) -> Result<Response, AuthError> {
    let user = request
        .extensions()
        .get::<AuthUser>()
        .ok_or(AuthError::MissingAuthHeader)?;

    if !user.role.is_admin() {
        return Err(AuthError::InsufficientPermissions);
    }

    Ok(next.run(request).await)
}

/// Middleware to require write permissions
pub async fn require_write(request: Request, next: Next) -> Result<Response, AuthError> {
    let user = request
        .extensions()
        .get::<AuthUser>()
        .ok_or(AuthError::MissingAuthHeader)?;

    if !user.role.can_write() {
        return Err(AuthError::InsufficientPermissions);
    }

    Ok(next.run(request).await)
}
