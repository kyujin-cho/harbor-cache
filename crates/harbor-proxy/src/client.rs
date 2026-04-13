//! Harbor upstream client

use bytes::Bytes;
use futures::Stream;
use reqwest::{Client, Response, StatusCode};
use serde::Deserialize;
use std::pin::Pin;
use tracing::{debug, info, warn};

use crate::error::ProxyError;

/// Type alias for a boxed stream of bytes
pub type ByteStream = Pin<Box<dyn Stream<Item = Result<Bytes, ProxyError>> + Send>>;

/// Harbor client configuration
#[derive(Clone, Debug)]
pub struct HarborClientConfig {
    /// Base URL of the upstream Harbor registry
    pub url: String,
    /// Registry/project name
    pub registry: String,
    /// Username for authentication
    pub username: Option<String>,
    /// Password for authentication
    pub password: Option<String>,
    /// Skip TLS certificate verification
    pub skip_tls_verify: bool,
}

/// Token response from Harbor
#[derive(Debug, Deserialize)]
struct TokenResponse {
    token: String,
    #[serde(default)]
    #[allow(dead_code)]
    expires_in: Option<u64>,
}

/// Harbor API client
pub struct HarborClient {
    config: HarborClientConfig,
    client: Client,
}

impl HarborClient {
    /// Create a new Harbor client
    pub fn new(config: HarborClientConfig) -> Result<Self, ProxyError> {
        let mut builder = Client::builder()
            .connect_timeout(std::time::Duration::from_secs(30))
            // No read_timeout — large blob uploads (multi-GB) can take minutes
            // to stream to the upstream, during which no response bytes arrive.
            // A 60 s read_timeout would kill every large push.  TCP keepalive
            // detects genuinely stalled connections at the transport level.
            .tcp_keepalive(std::time::Duration::from_secs(60));

        if config.skip_tls_verify {
            builder = builder.danger_accept_invalid_certs(true);
        }

        let client = builder.build()?;

        info!("Created Harbor client for {}", config.url);

        Ok(Self { config, client })
    }

    /// Parse WWW-Authenticate header and fetch token with proper scope
    async fn fetch_token_for_scope(&self, www_auth: &str) -> Result<String, ProxyError> {
        // Parse: Bearer realm="https://...",service="harbor-registry",scope="..."
        if !www_auth.starts_with("Bearer ") {
            return Err(ProxyError::InvalidResponse(
                "Expected Bearer authentication".to_string(),
            ));
        }

        let auth_params = &www_auth[7..];
        let mut realm = None;
        let mut service = None;
        let mut scope = None;

        // Parse key="value" pairs, respecting quotes (commas inside quotes are not separators)
        let mut current_key = String::new();
        let mut current_value = String::new();
        let mut in_value = false;
        let mut in_quotes = false;

        for ch in auth_params.chars() {
            match ch {
                '"' => {
                    in_quotes = !in_quotes;
                }
                '=' if !in_quotes && !in_value => {
                    in_value = true;
                }
                ',' if !in_quotes => {
                    // End of key-value pair
                    let key = current_key.trim();
                    let value = current_value.trim().trim_matches('"');
                    match key {
                        "realm" => realm = Some(value.to_string()),
                        "service" => service = Some(value.to_string()),
                        "scope" => scope = Some(value.to_string()),
                        _ => {}
                    }
                    current_key.clear();
                    current_value.clear();
                    in_value = false;
                }
                _ => {
                    if in_value {
                        current_value.push(ch);
                    } else {
                        current_key.push(ch);
                    }
                }
            }
        }

        // Handle last key-value pair
        if !current_key.is_empty() {
            let key = current_key.trim();
            let value = current_value.trim().trim_matches('"');
            match key {
                "realm" => realm = Some(value.to_string()),
                "service" => service = Some(value.to_string()),
                "scope" => scope = Some(value.to_string()),
                _ => {}
            }
        }

        let realm = realm.ok_or(ProxyError::InvalidResponse(
            "Missing realm in WWW-Authenticate".to_string(),
        ))?;

        // Build token request URL
        let mut url = realm.clone();
        let mut params = vec![];

        if let Some(svc) = service {
            params.push(format!("service={}", svc));
        }
        if let Some(scp) = scope {
            params.push(format!("scope={}", scp));
        }

        if !params.is_empty() {
            url = format!("{}?{}", url, params.join("&"));
        }

        debug!("Fetching token from: {}", url);

        let mut request = self.client.get(&url);

        // Add basic auth if credentials are provided
        if let (Some(username), Some(password)) = (&self.config.username, &self.config.password) {
            request = request.basic_auth(username, Some(password));
        }

        let response = request.send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ProxyError::UpstreamError {
                status: status.as_u16(),
                message: format!("Token request failed: {}", body),
            });
        }

        let token_response: TokenResponse = response.json().await?;

        Ok(format!("Bearer {}", token_response.token))
    }

    /// Make an authenticated request, handling 401 by getting a properly scoped token
    async fn authenticated_request(
        &self,
        method: &str,
        url: &str,
        headers: Vec<(&str, &str)>,
        body: Option<Bytes>,
    ) -> Result<Response, ProxyError> {
        // First attempt without token
        let mut request = match method {
            "GET" => self.client.get(url),
            "HEAD" => self.client.head(url),
            "PUT" => self.client.put(url),
            "POST" => self.client.post(url),
            "PATCH" => self.client.patch(url),
            _ => self.client.get(url),
        };

        for (key, value) in &headers {
            request = request.header(*key, *value);
        }

        if let Some(ref data) = body {
            request = request.body(data.clone());
        }

        let response = request.send().await?;

        // If unauthorized, get a token with the proper scope and retry
        if response.status() == StatusCode::UNAUTHORIZED {
            let www_auth = response
                .headers()
                .get("www-authenticate")
                .and_then(|h| h.to_str().ok())
                .ok_or(ProxyError::Unauthorized)?;

            debug!("Got 401, fetching token with scope from: {}", www_auth);

            let token = self.fetch_token_for_scope(www_auth).await?;

            // Retry with token
            let mut request = match method {
                "GET" => self.client.get(url),
                "HEAD" => self.client.head(url),
                "PUT" => self.client.put(url),
                "POST" => self.client.post(url),
                "PATCH" => self.client.patch(url),
                _ => self.client.get(url),
            };

            request = request.header("Authorization", &token);

            for (key, value) in &headers {
                request = request.header(*key, *value);
            }

            if let Some(data) = body {
                request = request.body(data);
            }

            return Ok(request.send().await?);
        }

        Ok(response)
    }

    /// Check if upstream is reachable
    pub async fn ping(&self) -> Result<bool, ProxyError> {
        let url = format!("{}/v2/", self.config.url);
        let response = self
            .authenticated_request("GET", &url, vec![], None)
            .await?;
        Ok(response.status().is_success())
    }

    /// Get the full repository path, handling the registry prefix
    fn full_repository(&self, repository: &str) -> String {
        // If repository already starts with registry prefix or equals registry, don't modify it
        if repository.starts_with(&format!("{}/", self.config.registry))
            || repository == self.config.registry
        {
            repository.to_string()
        } else {
            format!("{}/{}", self.config.registry, repository)
        }
    }

    /// Get a manifest from upstream
    pub async fn get_manifest(
        &self,
        repository: &str,
        reference: &str,
    ) -> Result<(Bytes, String, String), ProxyError> {
        let full_repo = self.full_repository(repository);
        let url = format!(
            "{}/v2/{}/manifests/{}",
            self.config.url, full_repo, reference
        );

        debug!("Fetching manifest: {}", url);

        let headers = vec![(
            "Accept",
            "application/vnd.oci.image.manifest.v1+json, \
             application/vnd.oci.image.index.v1+json, \
             application/vnd.docker.distribution.manifest.v2+json, \
             application/vnd.docker.distribution.manifest.list.v2+json, \
             application/vnd.docker.distribution.manifest.v1+prettyjws",
        )];

        let response = self
            .authenticated_request("GET", &url, headers, None)
            .await?;
        let status = response.status();

        if status == StatusCode::NOT_FOUND {
            return Err(ProxyError::NotFound(format!(
                "{}:{}",
                repository, reference
            )));
        }

        if !status.is_success() {
            return Err(ProxyError::UpstreamError {
                status: status.as_u16(),
                message: response.text().await.unwrap_or_default(),
            });
        }

        // Get content type and digest from headers
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|h| h.to_str().ok())
            .unwrap_or("application/vnd.oci.image.manifest.v1+json")
            .to_string();

        let digest = response
            .headers()
            .get("docker-content-digest")
            .and_then(|h| h.to_str().ok())
            .unwrap_or("")
            .to_string();

        let body = response.bytes().await?;

        Ok((body, content_type, digest))
    }

    /// Get a blob from upstream
    #[deprecated(note = "Use get_blob_stream() to avoid buffering entire blob in memory")]
    pub async fn get_blob(
        &self,
        repository: &str,
        digest: &str,
    ) -> Result<(Bytes, u64), ProxyError> {
        let full_repo = self.full_repository(repository);
        let url = format!("{}/v2/{}/blobs/{}", self.config.url, full_repo, digest);

        debug!("Fetching blob: {}", url);

        let response = self
            .authenticated_request("GET", &url, vec![], None)
            .await?;
        let status = response.status();

        if status == StatusCode::NOT_FOUND {
            return Err(ProxyError::NotFound(digest.to_string()));
        }

        if !status.is_success() {
            return Err(ProxyError::UpstreamError {
                status: status.as_u16(),
                message: response.text().await.unwrap_or_default(),
            });
        }

        let size = response
            .headers()
            .get("content-length")
            .and_then(|h| h.to_str().ok())
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        let body = response.bytes().await?;

        Ok((body, size))
    }

    /// Get a blob from upstream as a stream (avoids buffering entire blob in memory)
    pub async fn get_blob_stream(
        &self,
        repository: &str,
        digest: &str,
    ) -> Result<(ByteStream, u64), ProxyError> {
        let full_repo = self.full_repository(repository);
        let url = format!("{}/v2/{}/blobs/{}", self.config.url, full_repo, digest);

        debug!("Fetching blob stream: {}", url);

        let response = self
            .authenticated_request("GET", &url, vec![], None)
            .await?;
        let status = response.status();

        if status == StatusCode::NOT_FOUND {
            return Err(ProxyError::NotFound(digest.to_string()));
        }

        if !status.is_success() {
            let error_body = response.text().await.unwrap_or_default();
            // Log full upstream error server-side for debugging
            tracing::error!(
                "Upstream error for blob stream (status {}): {}",
                status.as_u16(),
                error_body
            );
            // Return generic message to client to avoid leaking upstream internals
            return Err(ProxyError::UpstreamError {
                status: status.as_u16(),
                message: format!("Upstream returned HTTP {}", status.as_u16()),
            });
        }

        let size = response
            .headers()
            .get("content-length")
            .and_then(|h| h.to_str().ok())
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        // Convert reqwest's BytesStream to our ByteStream type
        use futures::StreamExt;
        let stream = response.bytes_stream();
        let byte_stream: ByteStream =
            Box::pin(stream.map(|result| result.map_err(ProxyError::Http)));

        Ok((byte_stream, size))
    }

    /// Check if a blob exists
    pub async fn blob_exists(&self, repository: &str, digest: &str) -> Result<bool, ProxyError> {
        let full_repo = self.full_repository(repository);
        let url = format!("{}/v2/{}/blobs/{}", self.config.url, full_repo, digest);

        debug!("Checking blob existence: {}", url);

        let response = self
            .authenticated_request("HEAD", &url, vec![], None)
            .await?;

        Ok(response.status().is_success())
    }

    /// Get blob size and content type from upstream (HEAD request only, no download)
    pub async fn get_blob_size(
        &self,
        repository: &str,
        digest: &str,
    ) -> Result<(u64, String), ProxyError> {
        let full_repo = self.full_repository(repository);
        let url = format!("{}/v2/{}/blobs/{}", self.config.url, full_repo, digest);

        debug!("Getting blob size: {}", url);

        let response = self
            .authenticated_request("HEAD", &url, vec![], None)
            .await?;
        let status = response.status();

        if status == StatusCode::NOT_FOUND {
            return Err(ProxyError::NotFound(digest.to_string()));
        }

        if !status.is_success() {
            return Err(ProxyError::UpstreamError {
                status: status.as_u16(),
                message: "HEAD request failed".to_string(),
            });
        }

        let size = response
            .headers()
            .get("content-length")
            .and_then(|h| h.to_str().ok())
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|h| h.to_str().ok())
            .unwrap_or("application/octet-stream")
            .to_string();

        Ok((size, content_type))
    }

    /// Push a blob to upstream
    pub async fn push_blob(
        &self,
        repository: &str,
        digest: &str,
        data: Bytes,
    ) -> Result<(), ProxyError> {
        // First check if blob already exists upstream
        if self.blob_exists(repository, digest).await? {
            debug!("Blob {} already exists upstream, skipping upload", digest);
            return Ok(());
        }

        // Start upload
        let full_repo = self.full_repository(repository);
        let url = format!("{}/v2/{}/blobs/uploads/", self.config.url, full_repo);

        debug!("Starting blob upload to: {}", url);

        let response = self
            .authenticated_request("POST", &url, vec![], None)
            .await?;

        if !response.status().is_success() && response.status() != StatusCode::ACCEPTED {
            return Err(ProxyError::UpstreamError {
                status: response.status().as_u16(),
                message: response.text().await.unwrap_or_default(),
            });
        }

        // Get upload location
        let location = response
            .headers()
            .get("location")
            .and_then(|h| h.to_str().ok())
            .ok_or_else(|| ProxyError::InvalidResponse("Missing Location header".to_string()))?;

        // Complete upload with monolithic PUT (send all data in one request)
        // Location header may already contain query params (like ?_state=...)
        let separator = if location.contains('?') { '&' } else { '?' };
        let upload_url = if location.starts_with("http") {
            format!("{}{}digest={}", location, separator, digest)
        } else {
            format!(
                "{}{}{}digest={}",
                self.config.url, location, separator, digest
            )
        };

        debug!(
            "Completing blob upload: {} ({} bytes)",
            upload_url,
            data.len()
        );

        let headers = vec![("Content-Type", "application/octet-stream")];

        let response = self
            .authenticated_request("PUT", &upload_url, headers, Some(data))
            .await?;

        if !response.status().is_success() && response.status() != StatusCode::CREATED {
            return Err(ProxyError::UpstreamError {
                status: response.status().as_u16(),
                message: response.text().await.unwrap_or_default(),
            });
        }

        debug!("Blob {} uploaded successfully", digest);
        Ok(())
    }

    /// Push a blob to upstream as a stream (avoids buffering entire blob in memory)
    pub async fn push_blob_stream(
        &self,
        repository: &str,
        digest: &str,
        stream: ByteStream,
        size: u64,
    ) -> Result<(), ProxyError> {
        // First check if blob already exists upstream
        if self.blob_exists(repository, digest).await? {
            debug!("Blob {} already exists upstream, skipping upload", digest);
            return Ok(());
        }

        // Start upload
        let full_repo = self.full_repository(repository);
        let url = format!("{}/v2/{}/blobs/uploads/", self.config.url, full_repo);

        debug!("Starting blob upload to: {}", url);

        let response = self
            .authenticated_request("POST", &url, vec![], None)
            .await?;

        if !response.status().is_success() && response.status() != StatusCode::ACCEPTED {
            return Err(ProxyError::UpstreamError {
                status: response.status().as_u16(),
                message: response.text().await.unwrap_or_default(),
            });
        }

        // Get upload location
        let location = response
            .headers()
            .get("location")
            .and_then(|h| h.to_str().ok())
            .ok_or_else(|| ProxyError::InvalidResponse("Missing Location header".to_string()))?;

        // Complete upload with monolithic PUT (send all data in one request)
        // Location header may already contain query params (like ?_state=...)
        let separator = if location.contains('?') { '&' } else { '?' };
        let upload_url = if location.starts_with("http") {
            format!("{}{}digest={}", location, separator, digest)
        } else {
            format!(
                "{}{}{}digest={}",
                self.config.url, location, separator, digest
            )
        };

        debug!("Completing blob upload: {} ({} bytes)", upload_url, size);

        // Pre-fetch a push-scoped token before streaming the body.
        // A streaming body can only be consumed once — if the upstream returns
        // 401 mid-upload (e.g. because the token had only `pull` scope) Harbor
        // closes the connection while we are still writing, producing
        // `hyper::Error(IncompleteMessage)`.
        //
        // Instead, probe `/v2/` without credentials to obtain the realm/service
        // from the WWW-Authenticate header, then inject the explicit `pull,push`
        // scope for this repository before fetching the token.
        let push_token = {
            let v2_url = format!("{}/v2/", self.config.url);
            let probe = self.client.get(&v2_url).send().await?;
            if probe.status() == StatusCode::UNAUTHORIZED {
                if let Some(www_auth) = probe
                    .headers()
                    .get("www-authenticate")
                    .and_then(|h| h.to_str().ok())
                {
                    let scope = format!("repository:{}:pull,push", full_repo);
                    // If the challenge already carries a scope field, replace it;
                    // otherwise append one so `fetch_token_for_scope` picks it up.
                    let www_auth_with_push_scope = if let Some(start) = www_auth.find("scope=\"") {
                        // Replace the existing scope="..." value with our push scope
                        let after_open = start + 7; // length of `scope="`
                        if let Some(end_offset) = www_auth[after_open..].find('"') {
                            let end = after_open + end_offset;
                            format!(
                                "{}scope=\"{}\"{}",
                                &www_auth[..start],
                                scope,
                                &www_auth[end + 1..]
                            )
                        } else {
                            format!("{},scope=\"{}\"", www_auth, scope)
                        }
                    } else {
                        format!("{},scope=\"{}\"", www_auth, scope)
                    };
                    Some(
                        self.fetch_token_for_scope(&www_auth_with_push_scope)
                            .await?,
                    )
                } else {
                    None
                }
            } else {
                None
            }
        };

        // Convert ByteStream to reqwest::Body
        use futures::TryStreamExt;
        let reqwest_stream = stream.map_err(|e| std::io::Error::other(e.to_string()));
        let body = reqwest::Body::wrap_stream(reqwest_stream);

        let size_str = size.to_string();
        let mut put_request = self
            .client
            .put(&upload_url)
            .header("Content-Type", "application/octet-stream")
            .header("Content-Length", size_str.as_str())
            .body(body);

        if let Some(token) = push_token {
            put_request = put_request.header("Authorization", &token);
        }

        let response = put_request.send().await?;

        if !response.status().is_success() && response.status() != StatusCode::CREATED {
            return Err(ProxyError::UpstreamError {
                status: response.status().as_u16(),
                message: response.text().await.unwrap_or_default(),
            });
        }

        debug!("Blob {} uploaded successfully", digest);
        Ok(())
    }

    // ==================== Chunked Upload API ====================
    // These methods mirror the OCI chunked upload protocol to upstream,
    // enabling tee-streaming where data flows to both local storage and
    // upstream simultaneously.

    /// Start an upload session on upstream. Returns (location_url, auth_token).
    pub async fn start_upstream_upload(
        &self,
        repository: &str,
    ) -> Result<(String, Option<String>), ProxyError> {
        let full_repo = self.full_repository(repository);
        let url = format!("{}/v2/{}/blobs/uploads/", self.config.url, full_repo);

        debug!("Starting upstream upload session: {}", url);

        let response = self
            .authenticated_request("POST", &url, vec![], None)
            .await?;

        if !response.status().is_success() && response.status() != StatusCode::ACCEPTED {
            return Err(ProxyError::UpstreamError {
                status: response.status().as_u16(),
                message: response.text().await.unwrap_or_default(),
            });
        }

        // Extract auth token from the request that succeeded (for reuse)
        // We'll re-fetch when needed, but capture the location
        let location = response
            .headers()
            .get("location")
            .and_then(|h| h.to_str().ok())
            .ok_or_else(|| ProxyError::InvalidResponse("Missing Location header".to_string()))?;

        let location_url = if location.starts_with("http") {
            location.to_string()
        } else {
            format!("{}{}", self.config.url, location)
        };

        Ok((location_url, None))
    }

    /// PATCH data to an upstream upload session. Returns the updated location URL.
    pub async fn patch_upstream_upload(
        &self,
        location_url: &str,
        data: Bytes,
    ) -> Result<String, ProxyError> {
        debug!(
            "PATCHing {} bytes to upstream upload: {}",
            data.len(),
            location_url
        );

        let response = self
            .authenticated_request(
                "PATCH",
                location_url,
                vec![("Content-Type", "application/octet-stream")],
                Some(data),
            )
            .await?;

        if !response.status().is_success() && response.status() != StatusCode::ACCEPTED {
            return Err(ProxyError::UpstreamError {
                status: response.status().as_u16(),
                message: response.text().await.unwrap_or_default(),
            });
        }

        // Upstream may return an updated location
        let new_location = response
            .headers()
            .get("location")
            .and_then(|h| h.to_str().ok())
            .map(|loc| {
                if loc.starts_with("http") {
                    loc.to_string()
                } else {
                    format!("{}{}", self.config.url, loc)
                }
            })
            .unwrap_or_else(|| location_url.to_string());

        Ok(new_location)
    }

    /// Finalize an upstream upload with the digest.
    pub async fn finalize_upstream_upload(
        &self,
        location_url: &str,
        digest: &str,
    ) -> Result<(), ProxyError> {
        let separator = if location_url.contains('?') { '&' } else { '?' };
        let url = format!("{}{}digest={}", location_url, separator, digest);

        debug!("Finalizing upstream upload: {}", url);

        let response = self
            .authenticated_request("PUT", &url, vec![], None)
            .await?;

        let status = response.status();
        if !status.is_success() && status != StatusCode::CREATED {
            let message = response.text().await.unwrap_or_default();
            warn!("Upstream upload finalize failed ({}): {}", status, message);
            return Err(ProxyError::UpstreamError {
                status: status.as_u16(),
                message,
            });
        }

        Ok(())
    }

    /// Push a manifest to upstream
    pub async fn push_manifest(
        &self,
        repository: &str,
        reference: &str,
        data: Bytes,
        content_type: &str,
    ) -> Result<String, ProxyError> {
        let full_repo = self.full_repository(repository);
        let url = format!(
            "{}/v2/{}/manifests/{}",
            self.config.url, full_repo, reference
        );

        debug!("Pushing manifest to: {}", url);

        let headers = vec![("Content-Type", content_type)];

        let response = self
            .authenticated_request("PUT", &url, headers, Some(data))
            .await?;
        let status = response.status();

        if !status.is_success() && status != StatusCode::CREATED {
            let message = response.text().await.unwrap_or_default();
            warn!("Upstream manifest push failed ({}): {}", status, message);
            return Err(ProxyError::UpstreamError {
                status: status.as_u16(),
                message,
            });
        }

        // Get digest from response
        let digest = response
            .headers()
            .get("docker-content-digest")
            .and_then(|h| h.to_str().ok())
            .unwrap_or("")
            .to_string();

        Ok(digest)
    }
}
