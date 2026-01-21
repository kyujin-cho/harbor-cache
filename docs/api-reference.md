# Harbor Cache API Reference

Harbor Cache exposes two main APIs:
1. **OCI Distribution API** - Docker/OCI registry protocol (`/v2/...`)
2. **Management API** - Administration and monitoring (`/api/v1/...`)

## Authentication

### JWT Token Authentication

Most endpoints require authentication via JWT token in the `Authorization` header:

```
Authorization: Bearer <token>
```

Obtain a token via the login endpoint:

```bash
curl -X POST http://localhost:5001/api/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"admin"}'
```

Response:
```json
{
  "token": "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9...",
  "expires_in": 86400
}
```

### Token Payload

The JWT token contains:
```json
{
  "sub": "1",           // User ID
  "username": "admin",  // Username
  "role": "admin",      // User role
  "exp": 1234567890,    // Expiration timestamp
  "iat": 1234567890     // Issued at timestamp
}
```

---

## Health & Metrics

### GET /health

Health check endpoint. No authentication required.

**Response:**
```json
{
  "status": "healthy",
  "version": "0.1.0"
}
```

### GET /healthz

Alias for `/health`.

### GET /metrics

Prometheus metrics endpoint. No authentication required.

**Response:** (text/plain)
```
# TYPE harbor_cache_health_checks_total counter
harbor_cache_health_checks_total 42

# TYPE harbor_cache_requests_total counter
harbor_cache_requests_total 1234

# TYPE harbor_cache_hits_total counter
harbor_cache_hits_total 890

# TYPE harbor_cache_misses_total counter
harbor_cache_misses_total 344
```

---

## OCI Distribution API (v2)

Implements the [OCI Distribution Specification](https://github.com/opencontainers/distribution-spec).

### GET /v2/

Check API version. Returns empty JSON object with `Docker-Distribution-API-Version` header.

**Response Headers:**
```
Docker-Distribution-API-Version: registry/2.0
```

**Response Body:**
```json
{}
```

---

### Manifests

#### GET /v2/{name}/manifests/{reference}

Pull a manifest by tag or digest.

**Path Parameters:**
| Parameter | Description |
|-----------|-------------|
| name | Repository name (e.g., `library/nginx`) |
| reference | Tag name or digest (e.g., `latest` or `sha256:abc...`) |

**Request Headers:**
```
Accept: application/vnd.oci.image.manifest.v1+json,
        application/vnd.oci.image.index.v1+json,
        application/vnd.docker.distribution.manifest.v2+json,
        application/vnd.docker.distribution.manifest.list.v2+json
```

**Response Headers:**
```
Content-Type: application/vnd.docker.distribution.manifest.v2+json
Docker-Content-Digest: sha256:abc123...
Content-Length: 1234
```

**Response Body:** Manifest JSON

**Example:**
```bash
curl http://localhost:5001/v2/library/nginx/manifests/latest \
  -H "Accept: application/vnd.docker.distribution.manifest.v2+json"
```

#### HEAD /v2/{name}/manifests/{reference}

Check if manifest exists. Returns headers only.

**Response:** Same headers as GET, no body.

#### PUT /v2/{name}/manifests/{reference}

Push a manifest.

**Request Headers:**
```
Content-Type: application/vnd.docker.distribution.manifest.v2+json
```

**Request Body:** Manifest JSON

**Response:**
```
HTTP/1.1 201 Created
Location: /v2/library/nginx/manifests/sha256:abc123...
Docker-Content-Digest: sha256:abc123...
```

---

### Blobs

#### GET /v2/{name}/blobs/{digest}

Pull a blob by digest.

**Path Parameters:**
| Parameter | Description |
|-----------|-------------|
| name | Repository name |
| digest | Blob digest (e.g., `sha256:abc123...`) |

**Response Headers:**
```
Content-Type: application/octet-stream
Docker-Content-Digest: sha256:abc123...
Content-Length: 12345678
```

**Range Requests:**
```bash
curl http://localhost:5001/v2/library/nginx/blobs/sha256:abc... \
  -H "Range: bytes=0-1023"
```

#### HEAD /v2/{name}/blobs/{digest}

Check if blob exists. Returns headers only.

---

### Blob Uploads

#### POST /v2/{name}/blobs/uploads/

Initiate a blob upload.

**Query Parameters:**
| Parameter | Description |
|-----------|-------------|
| mount | (Optional) Digest to mount from another repository |
| from | (Optional) Source repository for mount |

**Response:**
```
HTTP/1.1 202 Accepted
Location: /v2/library/nginx/blobs/uploads/abc123-def456
Docker-Upload-UUID: abc123-def456
Range: 0-0
```

#### PATCH /v2/{name}/blobs/uploads/{session_id}

Upload a chunk of data.

**Request Headers:**
```
Content-Type: application/octet-stream
Content-Length: 1048576
Content-Range: 0-1048575
```

**Request Body:** Binary data

**Response:**
```
HTTP/1.1 202 Accepted
Location: /v2/library/nginx/blobs/uploads/abc123-def456
Docker-Upload-UUID: abc123-def456
Range: 0-1048575
```

#### PUT /v2/{name}/blobs/uploads/{session_id}

Complete a blob upload.

**Query Parameters:**
| Parameter | Description |
|-----------|-------------|
| digest | Final digest of the blob (required) |

**Request Body:** (Optional) Final chunk of data

**Response:**
```
HTTP/1.1 201 Created
Location: /v2/library/nginx/blobs/sha256:abc123...
Docker-Content-Digest: sha256:abc123...
```

---

## Management API

All management API endpoints require authentication.

### Authentication

#### POST /api/v1/auth/login

Authenticate and obtain JWT token.

**Request:**
```json
{
  "username": "admin",
  "password": "admin"
}
```

**Response (200):**
```json
{
  "token": "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9...",
  "expires_in": 86400
}
```

**Response (401):**
```json
{
  "errors": [{
    "code": "UNAUTHORIZED",
    "message": "Invalid credentials"
  }]
}
```

---

### Cache Management

#### GET /api/v1/cache/stats

Get cache statistics.

**Required Role:** Any authenticated user

**Response:**
```json
{
  "total_size": 1073741824,
  "total_size_human": "1.00 GB",
  "entry_count": 42,
  "manifest_count": 10,
  "blob_count": 32,
  "hit_count": 150,
  "miss_count": 50,
  "hit_rate": 0.75
}
```

#### DELETE /api/v1/cache

Clear all cache entries.

**Required Role:** admin

**Response:**
```json
{
  "cleared": 42
}
```

#### POST /api/v1/cache/cleanup

Trigger manual cache cleanup (eviction + retention enforcement).

**Required Role:** admin

**Response:**
```json
{
  "cleaned": 5
}
```

---

### User Management

#### GET /api/v1/users

List all users.

**Required Role:** admin

**Response:**
```json
[
  {
    "id": 1,
    "username": "admin",
    "role": "admin",
    "created_at": "2024-01-15T10:30:00Z",
    "updated_at": "2024-01-15T10:30:00Z"
  },
  {
    "id": 2,
    "username": "reader",
    "role": "read-only",
    "created_at": "2024-01-16T14:20:00Z",
    "updated_at": "2024-01-16T14:20:00Z"
  }
]
```

#### GET /api/v1/users/{id}

Get a specific user.

**Required Role:** admin

**Response:**
```json
{
  "id": 1,
  "username": "admin",
  "role": "admin",
  "created_at": "2024-01-15T10:30:00Z",
  "updated_at": "2024-01-15T10:30:00Z"
}
```

#### POST /api/v1/users

Create a new user.

**Required Role:** admin

**Request:**
```json
{
  "username": "newuser",
  "password": "secretpassword",
  "role": "read-write"
}
```

**Response (201):**
```json
{
  "id": 3,
  "username": "newuser",
  "role": "read-write",
  "created_at": "2024-01-17T09:00:00Z",
  "updated_at": "2024-01-17T09:00:00Z"
}
```

#### PUT /api/v1/users/{id}

Update a user.

**Required Role:** admin

**Request:**
```json
{
  "role": "admin",
  "password": "newpassword"
}
```

All fields are optional. Only provided fields are updated.

**Response:**
```json
{
  "id": 3,
  "username": "newuser",
  "role": "admin",
  "created_at": "2024-01-17T09:00:00Z",
  "updated_at": "2024-01-17T10:00:00Z"
}
```

#### DELETE /api/v1/users/{id}

Delete a user.

**Required Role:** admin

**Response:** 204 No Content

---

### Configuration

#### GET /api/v1/config

Get all configuration entries.

**Required Role:** admin

**Response:**
```json
[
  {
    "key": "cache.max_size",
    "value": "10737418240",
    "updated_at": "2024-01-15T10:30:00Z"
  },
  {
    "key": "cache.retention_days",
    "value": "30",
    "updated_at": "2024-01-15T10:30:00Z"
  }
]
```

#### GET /api/v1/config/{key}

Get a specific configuration entry.

**Required Role:** admin

**Response:**
```json
{
  "key": "cache.max_size",
  "value": "10737418240",
  "updated_at": "2024-01-15T10:30:00Z"
}
```

#### PUT /api/v1/config

Update configuration entries.

**Required Role:** admin

**Request:**
```json
{
  "entries": [
    {"key": "cache.max_size", "value": "21474836480"},
    {"key": "cache.retention_days", "value": "60"}
  ]
}
```

**Response:**
```json
{
  "updated": 2
}
```

#### DELETE /api/v1/config/{key}

Delete a configuration entry.

**Required Role:** admin

**Response:** 204 No Content

---

## Error Responses

All errors follow the OCI Distribution Spec error format:

```json
{
  "errors": [
    {
      "code": "ERROR_CODE",
      "message": "Human readable message",
      "detail": "Optional additional details"
    }
  ]
}
```

### Error Codes

| Code | HTTP Status | Description |
|------|-------------|-------------|
| UNAUTHORIZED | 401 | Authentication required or failed |
| DENIED | 403 | Permission denied |
| NAME_UNKNOWN | 404 | Repository not found |
| MANIFEST_UNKNOWN | 404 | Manifest not found |
| BLOB_UNKNOWN | 404 | Blob not found |
| DIGEST_INVALID | 400 | Invalid digest format or mismatch |
| SIZE_INVALID | 400 | Content size mismatch |
| UNSUPPORTED | 415 | Unsupported operation or media type |

---

## Rate Limiting

Harbor Cache does not implement rate limiting. For production deployments, use a reverse proxy (nginx, HAProxy) for rate limiting.

---

## CORS

CORS is enabled for all origins by default. Configure your reverse proxy for stricter CORS policies in production.
