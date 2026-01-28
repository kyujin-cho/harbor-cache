//! API error types

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ApiError {
    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Unauthorized")]
    Unauthorized,

    #[error("Forbidden")]
    Forbidden,

    #[error("Method not allowed")]
    MethodNotAllowed,

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Core error: {0}")]
    Core(#[from] harbor_core::CoreError),

    #[error("Database error: {0}")]
    Database(#[from] harbor_db::DbError),

    #[error("Auth error: {0}")]
    Auth(#[from] harbor_auth::AuthError),

    #[error("Storage error: {0}")]
    Storage(#[from] harbor_storage::StorageError),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code, message) = match &self {
            ApiError::NotFound(msg) => (StatusCode::NOT_FOUND, "NOT_FOUND", msg.clone()),
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, "BAD_REQUEST", msg.clone()),
            ApiError::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "UNAUTHORIZED",
                "Unauthorized".to_string(),
            ),
            ApiError::Forbidden => (StatusCode::FORBIDDEN, "FORBIDDEN", "Forbidden".to_string()),
            ApiError::MethodNotAllowed => (
                StatusCode::METHOD_NOT_ALLOWED,
                "METHOD_NOT_ALLOWED",
                "Method not allowed".to_string(),
            ),
            ApiError::Internal(msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "INTERNAL_ERROR",
                msg.clone(),
            ),
            ApiError::Core(e) => match e {
                harbor_core::CoreError::NotFound(msg) => {
                    (StatusCode::NOT_FOUND, "NOT_FOUND", msg.clone())
                }
                harbor_core::CoreError::BadRequest(msg) => {
                    (StatusCode::BAD_REQUEST, "BAD_REQUEST", msg.clone())
                }
                harbor_core::CoreError::InvalidDigest(msg) => {
                    (StatusCode::BAD_REQUEST, "DIGEST_INVALID", msg.clone())
                }
                _ => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "INTERNAL_ERROR",
                    e.to_string(),
                ),
            },
            ApiError::Database(e) => match e {
                harbor_db::DbError::NotFound(msg) => {
                    (StatusCode::NOT_FOUND, "NOT_FOUND", msg.clone())
                }
                _ => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "DATABASE_ERROR",
                    e.to_string(),
                ),
            },
            ApiError::Auth(e) => {
                let status = match e {
                    harbor_auth::AuthError::InsufficientPermissions => StatusCode::FORBIDDEN,
                    _ => StatusCode::UNAUTHORIZED,
                };
                (status, "AUTH_ERROR", e.to_string())
            }
            ApiError::Storage(e) => match e {
                harbor_storage::StorageError::NotFound(msg) => {
                    (StatusCode::NOT_FOUND, "NOT_FOUND", msg.clone())
                }
                _ => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "STORAGE_ERROR",
                    e.to_string(),
                ),
            },
        };

        // OCI Distribution spec error format
        let body = axum::Json(json!({
            "errors": [{
                "code": code,
                "message": message,
                "detail": null
            }]
        }));

        (status, body).into_response()
    }
}
