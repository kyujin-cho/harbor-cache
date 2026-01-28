//! OCI Distribution API routes

use axum::{
    Router,
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, head, patch, post, put},
};
use bytes::Bytes;
use serde::Deserialize;
use tracing::debug;

use crate::error::ApiError;
use crate::state::AppState;

/// Query parameters for blob upload completion
#[derive(Deserialize)]
pub struct UploadCompleteQuery {
    digest: Option<String>,
}

/// Query parameters for blob mount
#[derive(Deserialize)]
pub struct MountQuery {
    mount: Option<String>,
    from: Option<String>,
}

// ==================== Version Check ====================

/// GET /v2/ - Version check
async fn version_check() -> Response {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/json")],
        "{}",
    )
        .into_response()
}

// ==================== Routes ====================

/// Create registry routes
pub fn routes() -> Router<AppState> {
    Router::new()
        // Version check
        .route("/v2/", get(version_check))
        // Manifests (using wildcard to capture multi-segment repo names like library/alpine)
        .route("/v2/{*path}", get(handle_get_or_head_request))
        .route("/v2/{*path}", head(handle_get_or_head_request))
        .route("/v2/{*path}", put(handle_put_request))
        .route("/v2/{*path}", post(handle_post_request))
        .route("/v2/{*path}", patch(handle_patch_request))
}

/// Parse a path to extract repository name and operation details
fn parse_registry_path(path: &str) -> Option<RegistryRequest> {
    // Paths are like:
    // - library/alpine/manifests/latest
    // - library/alpine/blobs/sha256:...
    // - library/alpine/blobs/uploads/
    // - library/alpine/blobs/uploads/{session_id}

    // Find the last meaningful segment type
    if let Some(idx) = path.rfind("/manifests/") {
        let name = &path[..idx];
        let reference = &path[idx + 11..]; // len("/manifests/")
        return Some(RegistryRequest::Manifest {
            name: name.to_string(),
            reference: reference.to_string(),
        });
    }

    if let Some(idx) = path.rfind("/blobs/uploads/") {
        let name = &path[..idx];
        let session_id = &path[idx + 15..]; // len("/blobs/uploads/")
        if session_id.is_empty() {
            return Some(RegistryRequest::StartUpload {
                name: name.to_string(),
            });
        } else {
            return Some(RegistryRequest::Upload {
                name: name.to_string(),
                session_id: session_id.to_string(),
            });
        }
    }

    if let Some(idx) = path.rfind("/blobs/") {
        let name = &path[..idx];
        let digest = &path[idx + 7..]; // len("/blobs/")
        return Some(RegistryRequest::Blob {
            name: name.to_string(),
            digest: digest.to_string(),
        });
    }

    None
}

enum RegistryRequest {
    Manifest { name: String, reference: String },
    Blob { name: String, digest: String },
    StartUpload { name: String },
    Upload { name: String, session_id: String },
}

/// Handle GET and HEAD requests
async fn handle_get_or_head_request(
    State(state): State<AppState>,
    Path(path): Path<String>,
    method: axum::http::Method,
) -> Result<Response, ApiError> {
    let req = parse_registry_path(&path).ok_or_else(|| ApiError::NotFound(path.clone()))?;

    match req {
        RegistryRequest::Manifest { name, reference } => {
            if method == axum::http::Method::HEAD {
                debug!("HEAD manifest: {}:{}", name, reference);
                let result = state.registry.manifest_exists(&name, &reference).await?;
                match result {
                    Some((content_type, digest, size)) => {
                        let mut response = StatusCode::OK.into_response();
                        let headers = response.headers_mut();
                        headers.insert(
                            header::CONTENT_TYPE,
                            HeaderValue::from_str(&content_type).unwrap(),
                        );
                        headers.insert(header::CONTENT_LENGTH, HeaderValue::from(size as u64));
                        headers.insert(
                            "Docker-Content-Digest",
                            HeaderValue::from_str(&digest).unwrap(),
                        );
                        Ok(response)
                    }
                    None => Err(ApiError::NotFound(format!("{}:{}", name, reference))),
                }
            } else {
                debug!("GET manifest: {}:{}", name, reference);
                let (data, content_type, digest) =
                    state.registry.get_manifest(&name, &reference).await?;
                let mut response = (StatusCode::OK, data).into_response();
                let headers = response.headers_mut();
                headers.insert(
                    header::CONTENT_TYPE,
                    HeaderValue::from_str(&content_type).unwrap(),
                );
                headers.insert(
                    "Docker-Content-Digest",
                    HeaderValue::from_str(&digest).unwrap(),
                );
                Ok(response)
            }
        }
        RegistryRequest::Blob { name, digest } => {
            if method == axum::http::Method::HEAD {
                debug!("HEAD blob: {}", digest);
                let size = state.registry.blob_exists(&name, &digest).await?;
                match size {
                    Some(s) => {
                        let mut response = StatusCode::OK.into_response();
                        let headers = response.headers_mut();
                        headers.insert(
                            header::CONTENT_TYPE,
                            HeaderValue::from_static("application/octet-stream"),
                        );
                        headers.insert(header::CONTENT_LENGTH, HeaderValue::from(s as u64));
                        headers.insert(
                            "Docker-Content-Digest",
                            HeaderValue::from_str(&digest).unwrap(),
                        );
                        Ok(response)
                    }
                    None => Err(ApiError::NotFound(digest)),
                }
            } else {
                debug!("GET blob: {}", digest);
                let (stream, size) = state.registry.get_blob(&name, &digest).await?;

                // Stream the blob data to the client (bounded memory usage)
                let body = axum::body::Body::from_stream(stream);
                let mut response = (StatusCode::OK, body).into_response();
                let headers = response.headers_mut();
                headers.insert(
                    header::CONTENT_TYPE,
                    HeaderValue::from_static("application/octet-stream"),
                );
                // Only set Content-Length when we have a known size.
                // When upstream omits Content-Length (size=0), omitting it here
                // lets axum use chunked transfer encoding automatically.
                if size > 0 {
                    headers.insert(
                        header::CONTENT_LENGTH,
                        HeaderValue::from(size),
                    );
                }
                headers.insert(
                    "Docker-Content-Digest",
                    HeaderValue::from_str(&digest).unwrap(),
                );
                Ok(response)
            }
        }
        RegistryRequest::Upload { name, session_id } => {
            debug!("GET upload status: {}", session_id);
            let session = state
                .registry
                .get_upload_session(&session_id)
                .await?
                .ok_or_else(|| ApiError::NotFound(format!("Upload session: {}", session_id)))?;
            let location = format!("/v2/{}/blobs/uploads/{}", name, session_id);
            let range = format!("0-{}", session.bytes_received);
            let mut response = StatusCode::NO_CONTENT.into_response();
            let headers = response.headers_mut();
            headers.insert(header::LOCATION, HeaderValue::from_str(&location).unwrap());
            headers.insert(
                "Docker-Upload-UUID",
                HeaderValue::from_str(&session_id).unwrap(),
            );
            headers.insert(header::RANGE, HeaderValue::from_str(&range).unwrap());
            Ok(response)
        }
        RegistryRequest::StartUpload { .. } => Err(ApiError::MethodNotAllowed),
    }
}

/// Handle PUT requests
async fn handle_put_request(
    State(state): State<AppState>,
    Path(path): Path<String>,
    Query(query): Query<UploadCompleteQuery>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let req = parse_registry_path(&path).ok_or_else(|| ApiError::NotFound(path.clone()))?;

    match req {
        RegistryRequest::Manifest { name, reference } => {
            debug!("PUT manifest: {}:{}", name, reference);
            let content_type = headers
                .get(header::CONTENT_TYPE)
                .and_then(|h| h.to_str().ok())
                .unwrap_or("application/vnd.oci.image.manifest.v1+json");
            let digest = state
                .registry
                .put_manifest(&name, &reference, content_type, body)
                .await?;
            let location = format!("/v2/{}/manifests/{}", name, digest);
            let mut response = StatusCode::CREATED.into_response();
            let resp_headers = response.headers_mut();
            resp_headers.insert(header::LOCATION, HeaderValue::from_str(&location).unwrap());
            resp_headers.insert(
                "Docker-Content-Digest",
                HeaderValue::from_str(&digest).unwrap(),
            );
            Ok(response)
        }
        RegistryRequest::Upload { name, session_id } => {
            let digest = query
                .digest
                .ok_or_else(|| ApiError::BadRequest("Missing digest parameter".to_string()))?;
            debug!("PUT upload: {} -> {}", session_id, digest);
            if !body.is_empty() {
                state.registry.append_upload(&session_id, body).await?;
            }
            state
                .registry
                .complete_upload(&name, &session_id, &digest)
                .await?;
            let location = format!("/v2/{}/blobs/{}", name, digest);
            let mut response = StatusCode::CREATED.into_response();
            let headers = response.headers_mut();
            headers.insert(header::LOCATION, HeaderValue::from_str(&location).unwrap());
            headers.insert(
                "Docker-Content-Digest",
                HeaderValue::from_str(&digest).unwrap(),
            );
            Ok(response)
        }
        _ => Err(ApiError::MethodNotAllowed),
    }
}

/// Handle POST requests
async fn handle_post_request(
    State(state): State<AppState>,
    Path(path): Path<String>,
    Query(query): Query<MountQuery>,
) -> Result<Response, ApiError> {
    let req = parse_registry_path(&path).ok_or_else(|| ApiError::NotFound(path.clone()))?;

    match req {
        RegistryRequest::StartUpload { name } => {
            // Check if this is a mount request
            if let (Some(mount_digest), Some(from)) = (query.mount, query.from) {
                debug!("Mount request: {} from {}", mount_digest, from);
                if state
                    .registry
                    .mount_blob(&name, &mount_digest, &from)
                    .await?
                {
                    let location = format!("/v2/{}/blobs/{}", name, mount_digest);
                    let mut response = StatusCode::CREATED.into_response();
                    let headers = response.headers_mut();
                    headers.insert(header::LOCATION, HeaderValue::from_str(&location).unwrap());
                    headers.insert(
                        "Docker-Content-Digest",
                        HeaderValue::from_str(&mount_digest).unwrap(),
                    );
                    return Ok(response);
                }
            }
            // Start a new upload session
            debug!("Starting upload for: {}", name);
            let session_id = state.registry.start_upload(&name).await?;
            let location = format!("/v2/{}/blobs/uploads/{}", name, session_id);
            let mut response = StatusCode::ACCEPTED.into_response();
            let headers = response.headers_mut();
            headers.insert(header::LOCATION, HeaderValue::from_str(&location).unwrap());
            headers.insert(
                "Docker-Upload-UUID",
                HeaderValue::from_str(&session_id).unwrap(),
            );
            headers.insert(header::RANGE, HeaderValue::from_static("0-0"));
            Ok(response)
        }
        _ => Err(ApiError::MethodNotAllowed),
    }
}

/// Handle PATCH requests
async fn handle_patch_request(
    State(state): State<AppState>,
    Path(path): Path<String>,
    body: Bytes,
) -> Result<Response, ApiError> {
    let req = parse_registry_path(&path).ok_or_else(|| ApiError::NotFound(path.clone()))?;

    match req {
        RegistryRequest::Upload { name, session_id } => {
            debug!("PATCH upload: {} ({} bytes)", session_id, body.len());
            let new_size = state.registry.append_upload(&session_id, body).await?;
            let location = format!("/v2/{}/blobs/uploads/{}", name, session_id);
            let range = format!("0-{}", new_size - 1);
            let mut response = StatusCode::ACCEPTED.into_response();
            let headers = response.headers_mut();
            headers.insert(header::LOCATION, HeaderValue::from_str(&location).unwrap());
            headers.insert(
                "Docker-Upload-UUID",
                HeaderValue::from_str(&session_id).unwrap(),
            );
            headers.insert(header::RANGE, HeaderValue::from_str(&range).unwrap());
            Ok(response)
        }
        _ => Err(ApiError::MethodNotAllowed),
    }
}
