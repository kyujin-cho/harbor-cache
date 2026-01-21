//! Harbor upstream client

use bytes::Bytes;
use reqwest::{Client, Response, StatusCode};
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

use crate::error::ProxyError;

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
    token: Arc<RwLock<Option<String>>>,
}

impl HarborClient {
    /// Create a new Harbor client
    pub fn new(config: HarborClientConfig) -> Result<Self, ProxyError> {
        let mut builder = Client::builder();

        if config.skip_tls_verify {
            builder = builder.danger_accept_invalid_certs(true);
        }

        let client = builder.build()?;

        info!("Created Harbor client for {}", config.url);

        Ok(Self {
            config,
            client,
            token: Arc::new(RwLock::new(None)),
        })
    }

    /// Get the registry URL for OCI operations
    #[allow(dead_code)]
    fn registry_url(&self) -> String {
        format!("{}/v2/{}", self.config.url, self.config.registry)
    }

    /// Authenticate with Harbor and get a token
    async fn authenticate(&self) -> Result<String, ProxyError> {
        info!("Authenticating with Harbor at {}", self.config.url);

        // Try to get the token endpoint from the API
        let response = self.client.get(format!("{}/v2/", self.config.url)).send().await?;

        if response.status() == StatusCode::UNAUTHORIZED {
            // Parse WWW-Authenticate header to get token endpoint
            if let Some(auth_header) = response.headers().get("www-authenticate") {
                if let Ok(header_str) = auth_header.to_str() {
                    if let Some(token) = self.fetch_token_from_auth_header(header_str).await? {
                        return Ok(token);
                    }
                }
            }
        }

        // Fall back to basic auth if no token endpoint
        if let (Some(username), Some(password)) = (&self.config.username, &self.config.password) {
            // Use basic auth directly
            debug!("Using basic auth for Harbor");
            return Ok(format!(
                "Basic {}",
                base64_encode(&format!("{}:{}", username, password))
            ));
        }

        Err(ProxyError::Unauthorized)
    }

    /// Parse WWW-Authenticate header and fetch token
    async fn fetch_token_from_auth_header(
        &self,
        header: &str,
    ) -> Result<Option<String>, ProxyError> {
        // Parse: Bearer realm="https://...",service="harbor-registry",scope="..."
        if !header.starts_with("Bearer ") {
            return Ok(None);
        }

        let parts: Vec<&str> = header[7..].split(',').collect();
        let mut realm = None;
        let mut service = None;
        let mut scope = None;

        for part in parts {
            let kv: Vec<&str> = part.splitn(2, '=').collect();
            if kv.len() == 2 {
                let key = kv[0].trim();
                let value = kv[1].trim().trim_matches('"');
                match key {
                    "realm" => realm = Some(value),
                    "service" => service = Some(value),
                    "scope" => scope = Some(value),
                    _ => {}
                }
            }
        }

        let realm = realm.ok_or(ProxyError::InvalidResponse(
            "Missing realm in WWW-Authenticate".to_string(),
        ))?;

        // Build token request URL
        let mut url = realm.to_string();
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
            return Err(ProxyError::TokenRefreshFailed);
        }

        let token_response: TokenResponse = response.json().await?;

        Ok(Some(format!("Bearer {}", token_response.token)))
    }

    /// Get authentication header, refreshing token if needed
    async fn get_auth_header(&self) -> Result<Option<String>, ProxyError> {
        // Check if we have a cached token
        {
            let token = self.token.read().await;
            if let Some(ref t) = *token {
                return Ok(Some(t.clone()));
            }
        }

        // Authenticate and cache token
        let token = self.authenticate().await?;
        {
            let mut cached = self.token.write().await;
            *cached = Some(token.clone());
        }

        Ok(Some(token))
    }

    /// Make an authenticated request
    async fn request(&self, url: &str) -> Result<Response, ProxyError> {
        let mut request = self.client.get(url);

        if let Some(auth) = self.get_auth_header().await? {
            request = request.header("Authorization", auth);
        }

        let response = request.send().await?;

        // If unauthorized, try refreshing token
        if response.status() == StatusCode::UNAUTHORIZED {
            debug!("Token expired, refreshing...");

            // Clear cached token
            {
                let mut cached = self.token.write().await;
                *cached = None;
            }

            // Retry with new token
            let mut request = self.client.get(url);
            if let Some(auth) = self.get_auth_header().await? {
                request = request.header("Authorization", auth);
            }

            return Ok(request.send().await?);
        }

        Ok(response)
    }

    /// Check if upstream is reachable
    pub async fn ping(&self) -> Result<bool, ProxyError> {
        let url = format!("{}/v2/", self.config.url);
        let response = self.request(&url).await?;
        Ok(response.status().is_success())
    }

    /// Get a manifest from upstream
    pub async fn get_manifest(
        &self,
        repository: &str,
        reference: &str,
    ) -> Result<(Bytes, String, String), ProxyError> {
        let url = format!(
            "{}/v2/{}/{}/manifests/{}",
            self.config.url, self.config.registry, repository, reference
        );

        debug!("Fetching manifest: {}", url);

        let mut request = self.client.get(&url);

        // Accept OCI and Docker manifest types
        request = request.header(
            "Accept",
            "application/vnd.oci.image.manifest.v1+json, \
             application/vnd.oci.image.index.v1+json, \
             application/vnd.docker.distribution.manifest.v2+json, \
             application/vnd.docker.distribution.manifest.list.v2+json, \
             application/vnd.docker.distribution.manifest.v1+prettyjws",
        );

        if let Some(auth) = self.get_auth_header().await? {
            request = request.header("Authorization", auth);
        }

        let response = request.send().await?;
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
    pub async fn get_blob(&self, repository: &str, digest: &str) -> Result<(Bytes, u64), ProxyError> {
        let url = format!(
            "{}/v2/{}/{}/blobs/{}",
            self.config.url, self.config.registry, repository, digest
        );

        debug!("Fetching blob: {}", url);

        let response = self.request(&url).await?;
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

    /// Check if a blob exists
    pub async fn blob_exists(&self, repository: &str, digest: &str) -> Result<bool, ProxyError> {
        let url = format!(
            "{}/v2/{}/{}/blobs/{}",
            self.config.url, self.config.registry, repository, digest
        );

        debug!("Checking blob existence: {}", url);

        let mut request = self.client.head(&url);

        if let Some(auth) = self.get_auth_header().await? {
            request = request.header("Authorization", auth);
        }

        let response = request.send().await?;

        Ok(response.status().is_success())
    }

    /// Push a blob to upstream
    pub async fn push_blob(
        &self,
        repository: &str,
        digest: &str,
        data: Bytes,
    ) -> Result<(), ProxyError> {
        // Start upload
        let url = format!(
            "{}/v2/{}/{}/blobs/uploads/",
            self.config.url, self.config.registry, repository
        );

        debug!("Starting blob upload to: {}", url);

        let mut request = self.client.post(&url);

        if let Some(auth) = self.get_auth_header().await? {
            request = request.header("Authorization", auth);
        }

        let response = request.send().await?;

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

        // Complete upload
        let upload_url = if location.starts_with("http") {
            format!("{}?digest={}", location, digest)
        } else {
            format!("{}{}?digest={}", self.config.url, location, digest)
        };

        debug!("Completing blob upload: {}", upload_url);

        let mut request = self.client
            .put(&upload_url)
            .header("Content-Type", "application/octet-stream")
            .body(data);

        if let Some(auth) = self.get_auth_header().await? {
            request = request.header("Authorization", auth);
        }

        let response = request.send().await?;

        if !response.status().is_success() && response.status() != StatusCode::CREATED {
            return Err(ProxyError::UpstreamError {
                status: response.status().as_u16(),
                message: response.text().await.unwrap_or_default(),
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
        let url = format!(
            "{}/v2/{}/{}/manifests/{}",
            self.config.url, self.config.registry, repository, reference
        );

        debug!("Pushing manifest to: {}", url);

        let mut request = self.client
            .put(&url)
            .header("Content-Type", content_type)
            .body(data);

        if let Some(auth) = self.get_auth_header().await? {
            request = request.header("Authorization", auth);
        }

        let response = request.send().await?;
        let status = response.status();

        if !status.is_success() && status != StatusCode::CREATED {
            return Err(ProxyError::UpstreamError {
                status: status.as_u16(),
                message: response.text().await.unwrap_or_default(),
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

/// Simple base64 encoding
fn base64_encode(input: &str) -> String {
    use std::io::Write;
    let mut buf = Vec::new();
    {
        let mut encoder =
            base64::write::EncoderWriter::new(&mut buf, &base64::engine::general_purpose::STANDARD);
        encoder.write_all(input.as_bytes()).unwrap();
    }
    String::from_utf8(buf).unwrap()
}
