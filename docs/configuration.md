# Harbor Cache Configuration Reference

Harbor Cache is configured via TOML files. Configuration can also be overridden via environment variables.

## Configuration File Location

By default, Harbor Cache looks for configuration at `config/default.toml`. Specify a custom path with:

```bash
harbor-cache --config /path/to/config.toml
```

## Configuration Sections

### [server]

Server binding configuration.

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `bind_address` | string | `"0.0.0.0"` | IP address to bind to |
| `port` | integer | `5000` | Port number to listen on |

**Example:**
```toml
[server]
bind_address = "0.0.0.0"
port = 5001
```

**Environment Variables:**
- `HARBOR_CACHE_BIND` - Override bind address
- `HARBOR_CACHE_PORT` - Override port

---

### [cache]

Cache behavior configuration.

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `max_size` | integer | `10737418240` (10 GB) | Maximum cache size in bytes |
| `retention_days` | integer | `30` | Days to retain cached entries |
| `eviction_policy` | string | `"lru"` | Eviction policy: `lru`, `lfu`, `fifo` |

**Example:**
```toml
[cache]
max_size = 21474836480        # 20 GB
retention_days = 60
eviction_policy = "lru"
```

**Eviction Policies:**

| Policy | Description |
|--------|-------------|
| `lru` | Least Recently Used - evicts entries not accessed recently |
| `lfu` | Least Frequently Used - evicts entries with lowest access count |
| `fifo` | First In First Out - evicts oldest entries first |

---

### [upstream]

Upstream Harbor registry configuration.

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `url` | string | (required) | Upstream Harbor URL |
| `registry` | string | `"library"` | Default registry/project name |
| `username` | string | (optional) | Authentication username |
| `password` | string | (optional) | Authentication password |
| `skip_tls_verify` | boolean | `false` | Skip TLS certificate verification |

**Example:**
```toml
[upstream]
url = "https://harbor.example.com"
registry = "library"
username = "admin"
password = "secret"
skip_tls_verify = false
```

**Security Note:** For self-signed certificates, set `skip_tls_verify = true`. In production, use proper TLS certificates.

---

### [storage]

Storage backend configuration.

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `backend` | string | `"local"` | Storage backend: `local` or `s3` |

**Example:**
```toml
[storage]
backend = "local"
```

---

### [storage.local]

Local filesystem storage configuration.

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `path` | string | `"./data/cache"` | Directory path for cache storage |

**Example:**
```toml
[storage.local]
path = "/var/lib/harbor-cache"
```

**Directory Structure:**
```
<path>/
├── blobs/
│   └── sha256/
│       ├── ab/
│       │   └── abc123...
│       └── cd/
│           └── cde456...
└── uploads/
    └── <session-id>
```

---

### [storage.s3]

S3-compatible storage configuration. Used when `storage.backend = "s3"`.

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `bucket` | string | `"harbor-cache"` | S3 bucket name |
| `region` | string | `"us-east-1"` | AWS region |
| `endpoint` | string | (optional) | Custom S3 endpoint (for MinIO) |
| `access_key` | string | (optional) | AWS access key ID |
| `secret_key` | string | (optional) | AWS secret access key |
| `prefix` | string | `""` | Object key prefix |
| `allow_http` | boolean | `false` | Allow HTTP (not HTTPS) connections |

**Example (AWS S3):**
```toml
[storage]
backend = "s3"

[storage.s3]
bucket = "my-harbor-cache"
region = "us-west-2"
access_key = "AKIAIOSFODNN7EXAMPLE"
secret_key = "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"
```

**Example (MinIO):**
```toml
[storage]
backend = "s3"

[storage.s3]
bucket = "harbor-cache"
region = "us-east-1"
endpoint = "http://localhost:9000"
access_key = "minioadmin"
secret_key = "minioadmin"
allow_http = true
```

**AWS Credentials:** If `access_key` and `secret_key` are not provided, the AWS SDK credential chain is used (environment variables, IAM roles, etc.).

---

### [database]

SQLite database configuration.

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `path` | string | `"./data/harbor-cache.db"` | Database file path |

**Example:**
```toml
[database]
path = "/var/lib/harbor-cache/harbor-cache.db"
```

**Note:** The database stores metadata only. Actual blob data is in the storage backend.

**Auto-creation:** Harbor Cache automatically creates the parent directory of the database path on startup (using `create_dir_all`). You do not need to manually create the directory beforehand.

---

### [auth]

Authentication configuration.

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `jwt_secret` | string | `"change-me-in-production"` | Secret key for JWT signing |
| `enabled` | boolean | `true` | Enable/disable authentication |

**Example:**
```toml
[auth]
jwt_secret = "your-secure-random-string-here"
enabled = true
```

**Security Note:**
- Change `jwt_secret` in production!
- Use a cryptographically random string (32+ characters)
- Disabling auth (`enabled = false`) allows anonymous access

**Generate a secure secret:**
```bash
openssl rand -base64 32
```

---

### [logging]

Logging configuration.

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `level` | string | `"info"` | Log level |
| `format` | string | `"pretty"` | Log format: `pretty` or `json` |

**Log Levels:**
- `trace` - Very verbose debugging
- `debug` - Debugging information
- `info` - General information
- `warn` - Warnings
- `error` - Errors only

**Example:**
```toml
[logging]
level = "debug"
format = "json"
```

**Environment Variable:**
- `RUST_LOG` - Override log level (e.g., `RUST_LOG=harbor_cache=debug`)

---

### [tls]

TLS/HTTPS configuration for secure connections.

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `enabled` | boolean | `false` | Enable TLS/HTTPS |
| `cert_path` | string | (required if enabled) | Path to TLS certificate file (PEM format) |
| `key_path` | string | (required if enabled) | Path to TLS private key file (PEM format) |

**Example:**
```toml
[tls]
enabled = true
cert_path = "/etc/harbor-cache/tls/server.crt"
key_path = "/etc/harbor-cache/tls/server.key"
```

**Supported Key Formats:**
- PKCS#1 RSA keys (BEGIN RSA PRIVATE KEY)
- PKCS#8 keys (BEGIN PRIVATE KEY)
- SEC1 EC keys (BEGIN EC PRIVATE KEY)

**Generate Self-Signed Certificate:**
```bash
# Generate private key and certificate
openssl req -x509 -newkey rsa:4096 -keyout key.pem -out cert.pem -days 365 -nodes \
  -subj "/CN=harbor-cache.example.com"

# Or with SAN for multiple domains
openssl req -x509 -newkey rsa:4096 -keyout key.pem -out cert.pem -days 365 -nodes \
  -subj "/CN=harbor-cache.example.com" \
  -addext "subjectAltName=DNS:harbor-cache.example.com,DNS:localhost,IP:127.0.0.1"
```

**Security Note:** In production, use certificates from a trusted Certificate Authority (CA) or your organization's internal CA.

---

## Complete Example Configuration

```toml
# Harbor Cache Configuration

[server]
bind_address = "0.0.0.0"
port = 5001

[cache]
max_size = 10737418240        # 10 GB
retention_days = 30
eviction_policy = "lru"

[upstream]
url = "https://harbor.example.com"
registry = "library"
username = "harbor-cache"
password = "service-account-password"
skip_tls_verify = false

[storage]
backend = "local"

[storage.local]
path = "/var/lib/harbor-cache/data"

[database]
path = "/var/lib/harbor-cache/harbor-cache.db"

[auth]
jwt_secret = "a-very-long-and-secure-random-string-here"
enabled = true

[logging]
level = "info"
format = "json"

[tls]
enabled = false
# cert_path = "/etc/harbor-cache/tls/server.crt"
# key_path = "/etc/harbor-cache/tls/server.key"
```

---

## Environment Variable Overrides

Some options can be overridden via environment variables:

| Variable | Configuration |
|----------|---------------|
| `HARBOR_CACHE_BIND` | `server.bind_address` |
| `HARBOR_CACHE_PORT` | `server.port` |
| `RUST_LOG` | `logging.level` |

**Example:**
```bash
HARBOR_CACHE_PORT=8080 RUST_LOG=debug ./harbor-cache --config config.toml
```

---

## Configuration Validation

Harbor Cache validates configuration on startup. Common validation errors:

| Error | Solution |
|-------|----------|
| `Invalid eviction policy` | Use `lru`, `lfu`, or `fifo` |
| `Invalid upstream URL` | Ensure URL is valid (include scheme) |
| `Cannot create storage directory` | Check permissions |
| `Cannot connect to database` | Check path permissions (parent directory is auto-created) |

---

## Runtime Configuration

Some settings can be modified at runtime via the Management API:

```bash
# Update cache retention
curl -X PUT http://localhost:5001/api/v1/config \
  -H "Authorization: Bearer <token>" \
  -H "Content-Type: application/json" \
  -d '{"entries": [{"key": "cache.retention_days", "value": "60"}]}'
```

**Note:** Runtime changes are stored in the database and take effect immediately. They do not modify the configuration file.
