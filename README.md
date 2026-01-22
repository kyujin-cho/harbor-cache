# Harbor Cache

A lightweight caching proxy for Harbor container registries, written in Rust.

Harbor Cache acts as an intermediary between Docker clients and an upstream Harbor registry, caching container images and artifacts locally to reduce bandwidth usage and improve pull times.

## Features

- **OCI Distribution Spec Compliant**: Full support for pulling and pushing container images
- **Multi-Architecture Support**: Handles manifest lists and OCI image indexes
- **Multiple Storage Backends**: Local disk or S3-compatible storage (AWS S3, MinIO)
- **Cache Management**: Configurable eviction policies (LRU, LFU, FIFO)
- **Web UI**: Dashboard for monitoring and management
- **REST API**: Full management API for automation
- **Authentication**: JWT-based auth with role-based access control
- **TLS/HTTPS Support**: Native TLS support with PEM certificates
- **Prometheus Metrics**: Built-in metrics endpoint for monitoring

## Quick Start

### Using Docker

```bash
# Build the image
docker build -t harbor-cache .

# Run with default configuration
docker run -p 5001:5001 \
  -v ./config:/app/config \
  -v ./data:/app/data \
  harbor-cache
```

### Using Docker Compose

```bash
docker compose up -d
```

### Building from Source

```bash
# Build
cargo build --release

# Run
./target/release/harbor-cache --config config/default.toml
```

## Configuration

Harbor Cache is configured via TOML files. See `config/default.toml` for all options.

### Key Configuration Options

```toml
[server]
bind_address = "0.0.0.0"
port = 5001

[cache]
max_size = 10737418240  # 10 GB
retention_days = 30
eviction_policy = "lru"  # lru, lfu, or fifo

[upstream]
url = "https://harbor.example.com"
registry = "library"
username = "admin"
password = "secret"
skip_tls_verify = false

[storage]
backend = "local"  # local or s3

[storage.local]
path = "./data/cache"

[storage.s3]
bucket = "harbor-cache"
region = "us-east-1"
endpoint = "http://localhost:9000"  # For MinIO
access_key = "minioadmin"
secret_key = "minioadmin"
allow_http = true

[auth]
jwt_secret = "change-me-in-production"
enabled = true

[tls]
enabled = false
# cert_path = "/path/to/cert.pem"
# key_path = "/path/to/key.pem"
```

### TLS/HTTPS Configuration

To enable HTTPS:

```toml
[tls]
enabled = true
cert_path = "/etc/harbor-cache/tls/server.crt"
key_path = "/etc/harbor-cache/tls/server.key"
```

Generate a self-signed certificate for development:

```bash
openssl req -x509 -newkey rsa:4096 -keyout key.pem -out cert.pem -days 365 -nodes \
  -subj "/CN=localhost" -addext "subjectAltName=DNS:localhost,IP:127.0.0.1"
```

## Usage

### Docker Client Configuration

Configure Docker to use Harbor Cache as a registry mirror:

```bash
# Pull through cache
docker pull localhost:5001/library/nginx:latest

# Push through cache (forwards to upstream)
docker tag myimage:latest localhost:5001/library/myimage:latest
docker push localhost:5001/library/myimage:latest
```

### Web UI

Access the management UI at `http://localhost:5001`

Default credentials: `admin` / `admin`

### API Endpoints

| Endpoint | Description |
|----------|-------------|
| `GET /health` | Health check |
| `GET /metrics` | Prometheus metrics |
| `GET /v2/` | OCI Distribution version check |
| `GET /v2/<name>/manifests/<ref>` | Pull manifest |
| `GET /v2/<name>/blobs/<digest>` | Pull blob |
| `POST /api/v1/auth/login` | Authenticate |
| `GET /api/v1/cache/stats` | Cache statistics |
| `DELETE /api/v1/cache` | Clear cache |
| `POST /api/v1/cache/cleanup` | Run cache cleanup |
| `GET /api/v1/users` | List users |
| `POST /api/v1/users` | Create user |
| `GET /api/v1/config` | Get configuration |
| `PUT /api/v1/config` | Update configuration |

### User Roles

- **admin**: Full access to all features
- **read-write**: Can pull and push images
- **read-only**: Can only pull images

## Development

### Prerequisites

- Rust 1.84+
- Node.js 18+ (for frontend)
- Docker (for testing)

### Building

```bash
# Backend
cargo build

# Frontend
cd frontend
npm install
npm run build
```

### Testing

```bash
# Start test Harbor instance
cd harbor-setup/harbor
docker compose up -d

# Run e2e tests
./tests/e2e-test.sh

# Run specific test suite
./tests/e2e-test.sh multiarch
```

### Project Structure

```
harbor-cache/
├── crates/
│   ├── harbor-cache/     # Main binary
│   ├── harbor-core/      # Core business logic
│   ├── harbor-storage/   # Storage backends
│   ├── harbor-proxy/     # Upstream client
│   ├── harbor-api/       # REST API routes
│   ├── harbor-auth/      # Authentication
│   └── harbor-db/        # Database layer
├── frontend/             # Vue.js web UI
├── config/               # Configuration files
├── static/               # Built frontend assets
└── tests/                # E2E tests
```

## Monitoring

### Prometheus Metrics

Metrics are exposed at `/metrics` in Prometheus format:

- `harbor_cache_health_checks_total` - Health check requests
- `harbor_cache_requests_total` - Total cache requests
- `harbor_cache_hits_total` - Cache hits
- `harbor_cache_misses_total` - Cache misses
- `harbor_cache_size_bytes` - Current cache size
- `harbor_cache_entries` - Number of cached entries

### Grafana Dashboard

Import the provided dashboard from `docs/grafana-dashboard.json` (coming soon).

## License

MIT License - see [LICENSE](LICENSE) for details.

## Contributing

Contributions are welcome! Please read our contributing guidelines before submitting PRs.
