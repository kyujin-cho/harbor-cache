# Harbor Cache Deployment Guide

This guide covers various deployment scenarios for Harbor Cache.

## Prerequisites

- Docker 20.10+ or Podman 4.0+
- Network access to upstream Harbor registry
- Storage for cache data (local disk or S3)

## Quick Start

### Binary Installation

```bash
# Download release binary
curl -LO https://github.com/lablup/harbor-cache/releases/latest/download/harbor-cache-linux-amd64
chmod +x harbor-cache-linux-amd64
mv harbor-cache-linux-amd64 /usr/local/bin/harbor-cache

# Create configuration
mkdir -p /etc/harbor-cache
cat > /etc/harbor-cache/config.toml << 'EOF'
[server]
bind_address = "0.0.0.0"
port = 5001

[upstream]
url = "https://harbor.example.com"
registry = "library"
username = "admin"
password = "secret"

[storage]
backend = "local"

[storage.local]
path = "/var/lib/harbor-cache/data"

[database]
path = "/var/lib/harbor-cache/harbor-cache.db"

[auth]
jwt_secret = "change-me-in-production"
enabled = true
EOF

# Run (the database parent directory is auto-created on startup;
# the storage directory for local backend is also auto-created)
harbor-cache --config /etc/harbor-cache/config.toml
```

### Docker Installation

```bash
# Pull image
docker pull ghcr.io/lablup/harbor-cache:latest

# Run with configuration
docker run -d \
  --name harbor-cache \
  -p 5001:5001 \
  -v /path/to/config:/app/config:ro \
  -v /path/to/data:/app/data \
  ghcr.io/lablup/harbor-cache:latest
```

### Docker Compose

```yaml
version: '3.8'

services:
  harbor-cache:
    image: ghcr.io/lablup/harbor-cache:latest
    container_name: harbor-cache
    ports:
      - "5001:5001"
    volumes:
      - ./config:/app/config:ro
      - harbor-cache-data:/app/data
    environment:
      - RUST_LOG=info
    restart: unless-stopped
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:5001/health"]
      interval: 30s
      timeout: 10s
      retries: 3

volumes:
  harbor-cache-data:
```

**Important:** The Docker image includes pre-built frontend assets in `/app/static` (built during the multi-stage Docker build). Do **not** mount a host directory over `/app/static`, as this would replace the baked-in web UI with whatever is on the host (potentially an empty directory).

---

## Systemd Service

Create `/etc/systemd/system/harbor-cache.service`:

```ini
[Unit]
Description=Harbor Cache
After=network.target

[Service]
Type=simple
User=harbor-cache
Group=harbor-cache
ExecStart=/usr/local/bin/harbor-cache --config /etc/harbor-cache/config.toml
Restart=always
RestartSec=5
Environment=RUST_LOG=info

# Security hardening
NoNewPrivileges=yes
ProtectSystem=strict
ProtectHome=yes
ReadWritePaths=/var/lib/harbor-cache
PrivateTmp=yes

[Install]
WantedBy=multi-user.target
```

Enable and start:

```bash
# Create user
useradd -r -s /bin/false harbor-cache

# Set permissions
chown -R harbor-cache:harbor-cache /var/lib/harbor-cache
chown -R harbor-cache:harbor-cache /etc/harbor-cache

# Enable service
systemctl daemon-reload
systemctl enable harbor-cache
systemctl start harbor-cache

# Check status
systemctl status harbor-cache
journalctl -u harbor-cache -f
```

---

## Kubernetes Deployment

### ConfigMap

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: harbor-cache-config
data:
  config.toml: |
    [server]
    bind_address = "0.0.0.0"
    port = 5001

    [cache]
    max_size = 10737418240
    retention_days = 30
    eviction_policy = "lru"

    [upstream]
    url = "https://harbor.example.com"
    registry = "library"
    skip_tls_verify = false

    [storage]
    backend = "local"

    [storage.local]
    path = "/data/cache"

    [database]
    path = "/data/harbor-cache.db"

    [auth]
    jwt_secret = "${JWT_SECRET}"
    enabled = true

    [logging]
    level = "info"
    format = "json"
```

### Secret

```yaml
apiVersion: v1
kind: Secret
metadata:
  name: harbor-cache-secrets
type: Opaque
stringData:
  upstream-username: admin
  upstream-password: harbor-secret
  jwt-secret: your-secure-jwt-secret-here
```

### Deployment

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: harbor-cache
  labels:
    app: harbor-cache
spec:
  replicas: 1
  selector:
    matchLabels:
      app: harbor-cache
  template:
    metadata:
      labels:
        app: harbor-cache
    spec:
      containers:
      - name: harbor-cache
        image: ghcr.io/lablup/harbor-cache:latest
        ports:
        - containerPort: 5001
        env:
        - name: RUST_LOG
          value: "info"
        volumeMounts:
        - name: config
          mountPath: /app/config
        - name: data
          mountPath: /data
        resources:
          requests:
            memory: "256Mi"
            cpu: "100m"
          limits:
            memory: "1Gi"
            cpu: "1000m"
        livenessProbe:
          httpGet:
            path: /health
            port: 5001
          initialDelaySeconds: 10
          periodSeconds: 30
        readinessProbe:
          httpGet:
            path: /health
            port: 5001
          initialDelaySeconds: 5
          periodSeconds: 10
      volumes:
      - name: config
        configMap:
          name: harbor-cache-config
      - name: data
        persistentVolumeClaim:
          claimName: harbor-cache-data
```

### Service

```yaml
apiVersion: v1
kind: Service
metadata:
  name: harbor-cache
spec:
  selector:
    app: harbor-cache
  ports:
  - port: 5001
    targetPort: 5001
  type: ClusterIP
```

### PersistentVolumeClaim

```yaml
apiVersion: v1
kind: PersistentVolumeClaim
metadata:
  name: harbor-cache-data
spec:
  accessModes:
    - ReadWriteOnce
  resources:
    requests:
      storage: 100Gi
  storageClassName: fast-ssd
```

---

## S3 Backend Deployment

For shared cache across multiple instances, use S3 storage.

### With AWS S3

```toml
[storage]
backend = "s3"

[storage.s3]
bucket = "my-harbor-cache"
region = "us-west-2"
# Credentials via IAM role or environment variables
```

### With MinIO

```yaml
# docker-compose.yml
version: '3.8'

services:
  harbor-cache:
    image: ghcr.io/lablup/harbor-cache:latest
    ports:
      - "5001:5001"
    volumes:
      - ./config:/app/config:ro
    environment:
      - RUST_LOG=info
    depends_on:
      - minio

  minio:
    image: minio/minio:latest
    ports:
      - "9000:9000"
      - "9001:9001"
    volumes:
      - minio-data:/data
    environment:
      - MINIO_ROOT_USER=minioadmin
      - MINIO_ROOT_PASSWORD=minioadmin
    command: server /data --console-address ":9001"

volumes:
  minio-data:
```

Configuration:
```toml
[storage]
backend = "s3"

[storage.s3]
bucket = "harbor-cache"
region = "us-east-1"
endpoint = "http://minio:9000"
access_key = "minioadmin"
secret_key = "minioadmin"
allow_http = true
```

---

## Docker Client Configuration

Configure Docker daemon to use Harbor Cache as a registry mirror.

### Option 1: Direct Pull

Pull directly specifying the cache URL:

```bash
docker pull localhost:5001/library/nginx:latest
```

### Option 2: Insecure Registry

Add to `/etc/docker/daemon.json`:

```json
{
  "insecure-registries": ["harbor-cache.example.com:5001"]
}
```

Restart Docker:
```bash
systemctl restart docker
```

### Option 3: With TLS (Native)

Harbor Cache supports native TLS without a reverse proxy.

**1. Generate TLS certificates:**

```bash
# Create certificate directory
mkdir -p /etc/harbor-cache/tls

# Generate self-signed certificate (development)
openssl req -x509 -newkey rsa:4096 \
  -keyout /etc/harbor-cache/tls/server.key \
  -out /etc/harbor-cache/tls/server.crt \
  -days 365 -nodes \
  -subj "/CN=harbor-cache.example.com" \
  -addext "subjectAltName=DNS:harbor-cache.example.com,DNS:localhost,IP:127.0.0.1"

# Set permissions
chmod 600 /etc/harbor-cache/tls/server.key
chmod 644 /etc/harbor-cache/tls/server.crt
```

**2. Configure Harbor Cache with TLS:**

```toml
[server]
bind_address = "0.0.0.0"
port = 5001

[tls]
enabled = true
cert_path = "/etc/harbor-cache/tls/server.crt"
key_path = "/etc/harbor-cache/tls/server.key"
```

**3. Add CA certificate to Docker trust store:**

For self-signed certificates:
```bash
# Create directory for Docker certificates
mkdir -p /etc/docker/certs.d/harbor-cache.example.com:5001

# Copy the certificate
cp /etc/harbor-cache/tls/server.crt \
   /etc/docker/certs.d/harbor-cache.example.com:5001/ca.crt

# Restart Docker
systemctl restart docker
```

**4. Test the connection:**

```bash
# Test with curl
curl -v --cacert /etc/harbor-cache/tls/server.crt \
  https://harbor-cache.example.com:5001/v2/

# Pull image through HTTPS cache
docker pull harbor-cache.example.com:5001/library/nginx:latest
```

---

## TLS with Let's Encrypt

For production environments, use Let's Encrypt certificates:

```bash
# Install certbot
apt install certbot

# Obtain certificate (standalone mode - stop Harbor Cache first)
certbot certonly --standalone -d harbor-cache.example.com

# Configure Harbor Cache
cat >> /etc/harbor-cache/config.toml << 'EOF'
[tls]
enabled = true
cert_path = "/etc/letsencrypt/live/harbor-cache.example.com/fullchain.pem"
key_path = "/etc/letsencrypt/live/harbor-cache.example.com/privkey.pem"
EOF

# Set up auto-renewal with reload
cat > /etc/letsencrypt/renewal-hooks/post/harbor-cache.sh << 'EOF'
#!/bin/bash
systemctl reload harbor-cache
EOF
chmod +x /etc/letsencrypt/renewal-hooks/post/harbor-cache.sh
```

---

## Reverse Proxy Setup

### Nginx

```nginx
upstream harbor-cache {
    server 127.0.0.1:5001;
    keepalive 32;
}

server {
    listen 443 ssl http2;
    server_name registry.example.com;

    ssl_certificate /etc/ssl/certs/registry.crt;
    ssl_certificate_key /etc/ssl/private/registry.key;

    client_max_body_size 0;  # Disable size limit for uploads

    location / {
        proxy_pass http://harbor-cache;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;

        # Timeouts for large blob uploads
        proxy_connect_timeout 300;
        proxy_send_timeout 300;
        proxy_read_timeout 300;

        # Buffering for large responses
        proxy_buffering off;
        proxy_request_buffering off;
    }
}
```

### Traefik

```yaml
# traefik.yml
http:
  routers:
    harbor-cache:
      rule: "Host(`registry.example.com`)"
      service: harbor-cache
      tls:
        certResolver: letsencrypt

  services:
    harbor-cache:
      loadBalancer:
        servers:
          - url: "http://harbor-cache:5001"
```

---

## Monitoring Setup

### Prometheus

Add to Prometheus configuration:

```yaml
scrape_configs:
  - job_name: 'harbor-cache'
    static_configs:
      - targets: ['harbor-cache:5001']
    metrics_path: /metrics
```

### Grafana Dashboard

Import the dashboard from `docs/grafana-dashboard.json` or create panels for:

- Cache hit rate
- Total requests
- Cache size
- Response latency

### Alerting Rules

```yaml
groups:
- name: harbor-cache
  rules:
  - alert: HarborCacheDown
    expr: up{job="harbor-cache"} == 0
    for: 5m
    labels:
      severity: critical
    annotations:
      summary: Harbor Cache is down

  - alert: HarborCacheLowHitRate
    expr: harbor_cache_hits_total / (harbor_cache_hits_total + harbor_cache_misses_total) < 0.5
    for: 30m
    labels:
      severity: warning
    annotations:
      summary: Harbor Cache hit rate is below 50%
```

---

## Backup and Recovery

### Database Backup

```bash
# Backup
sqlite3 /var/lib/harbor-cache/harbor-cache.db ".backup /backup/harbor-cache-$(date +%Y%m%d).db"

# Restore
cp /backup/harbor-cache-20240115.db /var/lib/harbor-cache/harbor-cache.db
```

### Cache Data

The cache can be rebuilt from upstream, so backup is optional. For faster recovery:

```bash
# Backup (local storage)
tar -czf /backup/cache-$(date +%Y%m%d).tar.gz /var/lib/harbor-cache/data

# Restore
tar -xzf /backup/cache-20240115.tar.gz -C /
```

---

## Troubleshooting

### Common Issues

**Cannot connect to upstream:**
```bash
# Test connectivity
curl -v https://harbor.example.com/v2/

# Check DNS
dig harbor.example.com

# Check TLS
openssl s_client -connect harbor.example.com:443
```

**Permission denied:**
```bash
# Check file permissions
ls -la /var/lib/harbor-cache/

# Fix permissions
chown -R harbor-cache:harbor-cache /var/lib/harbor-cache/
```

**Database locked:**
```bash
# Check for stale locks
fuser /var/lib/harbor-cache/harbor-cache.db

# Restart service
systemctl restart harbor-cache
```

### Debug Logging

```bash
RUST_LOG=debug harbor-cache --config config.toml
```

### Health Check

```bash
curl http://localhost:5001/health
curl http://localhost:5001/metrics
```
