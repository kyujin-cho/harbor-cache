//! Authentication error types

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AuthError {
    #[error("Invalid credentials")]
    InvalidCredentials,

    #[error("Invalid token")]
    InvalidToken,

    #[error("Token expired")]
    TokenExpired,

    #[error("Missing authorization header")]
    MissingAuthHeader,

    #[error("Invalid authorization header format")]
    InvalidAuthHeader,

    #[error("Insufficient permissions")]
    InsufficientPermissions,

    #[error("User not found")]
    UserNotFound,

    #[error("Password hashing error: {0}")]
    PasswordHash(String),

    #[error("JWT error: {0}")]
    Jwt(#[from] jsonwebtoken::errors::Error),
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            AuthError::InvalidCredentials => (StatusCode::UNAUTHORIZED, "Invalid credentials"),
            AuthError::InvalidToken => (StatusCode::UNAUTHORIZED, "Invalid token"),
            AuthError::TokenExpired => (StatusCode::UNAUTHORIZED, "Token expired"),
            AuthError::MissingAuthHeader => {
                (StatusCode::UNAUTHORIZED, "Missing authorization header")
            }
            AuthError::InvalidAuthHeader => (
                StatusCode::UNAUTHORIZED,
                "Invalid authorization header format",
            ),
            AuthError::InsufficientPermissions => {
                (StatusCode::FORBIDDEN, "Insufficient permissions")
            }
            AuthError::UserNotFound => (StatusCode::NOT_FOUND, "User not found"),
            AuthError::PasswordHash(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Internal error"),
            AuthError::Jwt(_) => (StatusCode::UNAUTHORIZED, "Invalid token"),
        };

        let body = axum::Json(json!({
            "error": message
        }));

        (status, body).into_response()
    }
}
