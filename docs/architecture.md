# Harbor Cache Architecture

## Overview

Harbor Cache is a lightweight caching proxy for Harbor container registries. It sits between Docker clients and an upstream Harbor registry, transparently caching container images to reduce bandwidth and improve pull performance.

```
┌─────────────┐     ┌──────────────┐     ┌─────────────────┐
│   Docker    │────▶│ Harbor Cache │────▶│ Upstream Harbor │
│   Client    │◀────│    Proxy     │◀────│    Registry     │
└─────────────┘     └──────────────┘     └─────────────────┘
                           │
                    ┌──────┴──────┐
                    │             │
               ┌────▼────┐  ┌─────▼─────┐
               │  Cache  │  │  Database │
               │ Storage │  │  (SQLite) │
               └─────────┘  └───────────┘
```

## Design Principles

1. **Transparency**: Acts as a drop-in replacement for direct Harbor access
2. **Performance**: Minimizes latency through local caching
3. **Reliability**: Graceful degradation when upstream is unavailable
4. **Simplicity**: Single binary deployment with minimal dependencies

## Crate Structure

The project is organized as a Cargo workspace with multiple crates:

```
harbor-cache/
├── crates/
│   ├── harbor-cache/     # Main binary
│   ├── harbor-core/      # Core business logic
│   ├── harbor-storage/   # Storage abstraction
│   ├── harbor-proxy/     # Upstream client
│   ├── harbor-api/       # REST API
│   ├── harbor-auth/      # Authentication
│   └── harbor-db/        # Database layer
└── frontend/             # Web UI
```

### Crate Responsibilities

#### harbor-cache (Binary)

The main entry point containing:
- CLI argument parsing (clap)
- Configuration loading
- Automatic creation of database parent directory before initialization
- Component initialization
- Server startup and shutdown

#### harbor-core

Core business logic including:
- `CacheManager`: Orchestrates caching operations
- `RegistryService`: Implements OCI Distribution operations
- Eviction policies (LRU, LFU, FIFO)
- Background cleanup tasks

#### harbor-storage

Storage abstraction layer:
- `StorageBackend` trait defining storage operations
- `LocalStorage`: File system storage implementation
- `S3Storage`: S3-compatible storage implementation
- Content-addressable storage with digest verification

#### harbor-proxy

Upstream Harbor client:
- `HarborClient`: HTTP client for Harbor API
- Token authentication flow
- TLS configuration (including skip-verify)
- Request/response streaming

#### harbor-api

Axum-based HTTP API:
- OCI Distribution Spec endpoints (`/v2/...`)
- Management API endpoints (`/api/v1/...`)
- Static file serving for Web UI
- Prometheus metrics endpoint

#### harbor-auth

Authentication and authorization:
- JWT token generation and validation
- Argon2 password hashing
- Role-based access control
- Auth middleware for Axum

#### harbor-db

Database layer (SQLite):
- Cache entry metadata
- User management
- Configuration storage
- Upload session tracking

## Data Flow

### Pull Operation (Cache Miss)

```
1. Client requests manifest: GET /v2/library/nginx/manifests/latest
2. Harbor Cache checks local cache (miss)
3. Harbor Cache fetches from upstream Harbor
4. Response is stored in cache (storage + metadata in DB)
5. Response is returned to client
```

### Pull Operation (Cache Hit)

```
1. Client requests manifest: GET /v2/library/nginx/manifests/latest
2. Harbor Cache checks local cache (hit)
3. Cache metadata is updated (access time, count)
4. Cached response is returned to client
```

### Push Operation

```
1. Client initiates upload: POST /v2/library/nginx/blobs/uploads/
2. Harbor Cache creates upload session
3. Client uploads chunks: PATCH /v2/.../uploads/<session>
4. Client completes upload: PUT /v2/.../uploads/<session>?digest=...
5. Harbor Cache verifies digest and stores blob
6. Harbor Cache forwards to upstream Harbor
```

## Storage Architecture

### Content-Addressable Storage

Blobs are stored using their digest as the identifier:

```
<base_path>/blobs/<algorithm>/<shard>/<hash>

Example:
./data/cache/blobs/sha256/ab/abc123def456...
```

The first two characters of the hash are used for sharding to avoid filesystem limitations with large numbers of files in a single directory.

### Upload Sessions

Chunked uploads are tracked in the database and stored temporarily:

```
<base_path>/uploads/<session_id>
```

Upon completion, the file is verified and moved to the content-addressable location.

## Cache Management

### Eviction Policies

| Policy | Description |
|--------|-------------|
| LRU | Least Recently Used - evicts entries not accessed recently |
| LFU | Least Frequently Used - evicts entries with lowest access count |
| FIFO | First In First Out - evicts oldest entries |

### Cleanup Process

The background cleanup task runs periodically to:

1. Remove entries exceeding retention period
2. Evict entries when cache size exceeds limit
3. Clean up orphaned upload sessions

## Security Model

### Authentication Flow

```
1. Client sends credentials to /api/v1/auth/login
2. Server validates credentials against database
3. Server returns JWT token (24-hour expiry)
4. Client includes token in Authorization header
5. Server validates token on each request
```

### Role-Based Access Control

| Role | Permissions |
|------|-------------|
| admin | Full access (users, config, cache management) |
| read-write | Pull and push images |
| read-only | Pull images only |

## High Availability Considerations

Harbor Cache is designed as a single-instance service. For high availability:

1. **Multiple Instances**: Deploy multiple instances behind a load balancer
2. **Shared Storage**: Use S3 backend for shared cache storage
3. **Separate Databases**: Each instance maintains its own SQLite database
4. **Stateless Design**: JWT tokens are self-contained

## Performance Characteristics

### Memory Usage

- Streaming for large blobs (no full buffering)
- Connection pooling for upstream requests
- Efficient async I/O with Tokio

### Disk I/O

- Content-addressable deduplication
- Sharded directory structure
- Atomic file operations

### Network

- HTTP/1.1 keep-alive connections
- Range request support for partial downloads
- Compression pass-through from upstream
