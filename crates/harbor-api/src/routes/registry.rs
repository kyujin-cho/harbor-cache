//! OCI Distribution API routes

use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, head, patch, post, put},
    Router,
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

// ==================== Manifest Operations ====================

/// GET /v2/:name/manifests/:reference
async fn get_manifest(
    State(state): State<AppState>,
    Path((name, reference)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    debug!("GET manifest: {}:{}", name, reference);

    let (data, content_type, digest) = state.registry.get_manifest(&name, &reference).await?;

    let mut response = (StatusCode::OK, data).into_response();
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_str(&content_type).unwrap());
    headers.insert("Docker-Content-Digest", HeaderValue::from_str(&digest).unwrap());

    Ok(response)
}

/// HEAD /v2/:name/manifests/:reference
async fn head_manifest(
    State(state): State<AppState>,
    Path((name, reference)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    debug!("HEAD manifest: {}:{}", name, reference);

    let result = state.registry.manifest_exists(&name, &reference).await?;

    match result {
        Some((content_type, digest, size)) => {
            let mut response = StatusCode::OK.into_response();
            let headers = response.headers_mut();
            headers.insert(header::CONTENT_TYPE, HeaderValue::from_str(&content_type).unwrap());
            headers.insert(header::CONTENT_LENGTH, HeaderValue::from(size as u64));
            headers.insert("Docker-Content-Digest", HeaderValue::from_str(&digest).unwrap());
            Ok(response)
        }
        None => Err(ApiError::NotFound(format!("{}:{}", name, reference))),
    }
}

/// PUT /v2/:name/manifests/:reference
async fn put_manifest(
    State(state): State<AppState>,
    Path((name, reference)): Path<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
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
    resp_headers.insert("Docker-Content-Digest", HeaderValue::from_str(&digest).unwrap());

    Ok(response)
}

// ==================== Blob Operations ====================

/// GET /v2/:name/blobs/:digest
async fn get_blob(
    State(state): State<AppState>,
    Path((name, digest)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    debug!("GET blob: {}", digest);

    let data = state.registry.get_blob(&name, &digest).await?;

    let mut response = (StatusCode::OK, data).into_response();
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/octet-stream"));
    headers.insert("Docker-Content-Digest", HeaderValue::from_str(&digest).unwrap());

    Ok(response)
}

/// HEAD /v2/:name/blobs/:digest
async fn head_blob(
    State(state): State<AppState>,
    Path((name, digest)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    debug!("HEAD blob: {}", digest);

    let size = state.registry.blob_exists(&name, &digest).await?;

    match size {
        Some(s) => {
            let mut response = StatusCode::OK.into_response();
            let headers = response.headers_mut();
            headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/octet-stream"));
            headers.insert(header::CONTENT_LENGTH, HeaderValue::from(s as u64));
            headers.insert("Docker-Content-Digest", HeaderValue::from_str(&digest).unwrap());
            Ok(response)
        }
        None => Err(ApiError::NotFound(digest)),
    }
}

// ==================== Upload Operations ====================

/// POST /v2/:name/blobs/uploads/
async fn start_upload(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(query): Query<MountQuery>,
) -> Result<Response, ApiError> {
    // Check if this is a mount request
    if let (Some(mount_digest), Some(from)) = (query.mount, query.from) {
        debug!("Mount request: {} from {}", mount_digest, from);

        if state.registry.mount_blob(&name, &mount_digest, &from).await? {
            let location = format!("/v2/{}/blobs/{}", name, mount_digest);
            let mut response = StatusCode::CREATED.into_response();
            let headers = response.headers_mut();
            headers.insert(header::LOCATION, HeaderValue::from_str(&location).unwrap());
            headers.insert("Docker-Content-Digest", HeaderValue::from_str(&mount_digest).unwrap());
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
    headers.insert("Docker-Upload-UUID", HeaderValue::from_str(&session_id).unwrap());
    headers.insert(header::RANGE, HeaderValue::from_static("0-0"));

    Ok(response)
}

/// PATCH /v2/:name/blobs/uploads/:session_id
async fn patch_upload(
    State(state): State<AppState>,
    Path((name, session_id)): Path<(String, String)>,
    body: Bytes,
) -> Result<Response, ApiError> {
    debug!("PATCH upload: {} ({} bytes)", session_id, body.len());

    let new_size = state.registry.append_upload(&session_id, body).await?;

    let location = format!("/v2/{}/blobs/uploads/{}", name, session_id);
    let range = format!("0-{}", new_size - 1);
    let mut response = StatusCode::ACCEPTED.into_response();
    let headers = response.headers_mut();
    headers.insert(header::LOCATION, HeaderValue::from_str(&location).unwrap());
    headers.insert("Docker-Upload-UUID", HeaderValue::from_str(&session_id).unwrap());
    headers.insert(header::RANGE, HeaderValue::from_str(&range).unwrap());

    Ok(response)
}

/// PUT /v2/:name/blobs/uploads/:session_id?digest=...
async fn complete_upload(
    State(state): State<AppState>,
    Path((name, session_id)): Path<(String, String)>,
    Query(query): Query<UploadCompleteQuery>,
    body: Bytes,
) -> Result<Response, ApiError> {
    let digest = query
        .digest
        .ok_or_else(|| ApiError::BadRequest("Missing digest parameter".to_string()))?;

    debug!("PUT upload: {} -> {}", session_id, digest);

    // Append final chunk if provided
    if !body.is_empty() {
        state.registry.append_upload(&session_id, body).await?;
    }

    // Complete the upload
    state.registry.complete_upload(&name, &session_id, &digest).await?;

    let location = format!("/v2/{}/blobs/{}", name, digest);
    let mut response = StatusCode::CREATED.into_response();
    let headers = response.headers_mut();
    headers.insert(header::LOCATION, HeaderValue::from_str(&location).unwrap());
    headers.insert("Docker-Content-Digest", HeaderValue::from_str(&digest).unwrap());

    Ok(response)
}

/// GET /v2/:name/blobs/uploads/:session_id
async fn get_upload_status(
    State(state): State<AppState>,
    Path((name, session_id)): Path<(String, String)>,
) -> Result<Response, ApiError> {
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
    headers.insert("Docker-Upload-UUID", HeaderValue::from_str(&session_id).unwrap());
    headers.insert(header::RANGE, HeaderValue::from_str(&range).unwrap());

    Ok(response)
}

// ==================== Routes ====================

/// Create registry routes
pub fn routes() -> Router<AppState> {
    Router::new()
        // Version check
        .route("/v2/", get(version_check))
        // Manifests
        .route("/v2/{name}/manifests/{reference}", get(get_manifest))
        .route("/v2/{name}/manifests/{reference}", head(head_manifest))
        .route("/v2/{name}/manifests/{reference}", put(put_manifest))
        // Blobs
        .route("/v2/{name}/blobs/{digest}", get(get_blob))
        .route("/v2/{name}/blobs/{digest}", head(head_blob))
        // Uploads
        .route("/v2/{name}/blobs/uploads/", post(start_upload))
        .route("/v2/{name}/blobs/uploads/{session_id}", get(get_upload_status))
        .route("/v2/{name}/blobs/uploads/{session_id}", patch(patch_upload))
        .route("/v2/{name}/blobs/uploads/{session_id}", put(complete_upload))
}
