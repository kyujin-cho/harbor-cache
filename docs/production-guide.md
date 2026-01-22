# Harbor Cache Production Guide

This guide covers deploying and operating Harbor Cache in production environments.

## Table of Contents

- [Pre-Deployment Checklist](#pre-deployment-checklist)
- [Performance Tuning](#performance-tuning)
- [Capacity Planning](#capacity-planning)
- [High Availability](#high-availability)
- [Backup and Recovery](#backup-and-recovery)
- [Monitoring and Alerting](#monitoring-and-alerting)
- [Log Analysis](#log-analysis)
- [Maintenance Operations](#maintenance-operations)
- [Disaster Recovery](#disaster-recovery)

## Pre-Deployment Checklist

Before deploying Harbor Cache to production, complete this checklist:

### Security

- [ ] TLS/HTTPS enabled with valid certificates
- [ ] Default admin password changed
- [ ] JWT secret configured with secure random value
- [ ] Network access properly restricted (firewall rules)
- [ ] Upstream credentials secured (not in version control)

### Configuration

- [ ] Appropriate cache size configured for available storage
- [ ] Retention period set based on usage patterns
- [ ] Eviction policy selected (LRU recommended for most cases)
- [ ] Logging configured for production (JSON format, appropriate level)

### Infrastructure

- [ ] Adequate storage provisioned (2x expected cache size recommended)
- [ ] Memory allocation appropriate (256MB minimum, 1GB+ recommended)
- [ ] Network bandwidth sufficient for expected traffic
- [ ] DNS records configured
- [ ] Load balancer configured (if applicable)

### Monitoring

- [ ] Prometheus metrics endpoint accessible
- [ ] Alerting rules configured
- [ ] Log aggregation configured
- [ ] Dashboard created for key metrics

## Performance Tuning

### System Resources

**CPU:**
- Harbor Cache is I/O-bound, not CPU-bound
- 1-2 CPU cores sufficient for most workloads
- More cores help with concurrent connections

**Memory:**
- Minimum: 256 MB
- Recommended: 1 GB
- High-traffic: 2-4 GB

Memory is used for:
- Connection buffers
- In-flight request handling
- Database connection pool

**Disk I/O:**
- SSD strongly recommended for local storage
- NVMe for high-performance requirements
- Cache size affects database size and lookup performance

### Network Optimization

**Connection Settings:**

```toml
# Increase for high-traffic environments
[server]
bind_address = "0.0.0.0"
port = 5001
```

**Upstream Connection:**

```toml
[upstream]
url = "https://harbor.example.com"
# Connection pool is managed automatically
# Timeout is 30s by default
```

**Reverse Proxy (Nginx):**

```nginx
upstream harbor-cache {
    server 127.0.0.1:5001;
    keepalive 32;  # Connection pool to backend
}

server {
    listen 443 ssl http2;

    # Disable buffering for streaming
    proxy_buffering off;
    proxy_request_buffering off;

    # Increase timeouts for large uploads
    proxy_connect_timeout 300;
    proxy_send_timeout 300;
    proxy_read_timeout 300;

    # No body size limit for container images
    client_max_body_size 0;

    location / {
        proxy_pass http://harbor-cache;
        proxy_http_version 1.1;
        proxy_set_header Connection "";
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
    }
}
```

### Cache Configuration

**Eviction Policies:**

| Policy | Best For | Characteristics |
|--------|----------|-----------------|
| `lru` | General use | Evicts least recently accessed items |
| `lfu` | Hot/cold workloads | Evicts least frequently accessed items |
| `fifo` | Simple rotation | Evicts oldest items regardless of access |

**Recommended for production:**

```toml
[cache]
max_size = 107374182400  # 100 GB
retention_days = 30
eviction_policy = "lru"
```

### Database Optimization

SQLite is used for metadata. For large installations:

```bash
# Optimize database periodically
sqlite3 /var/lib/harbor-cache/harbor-cache.db "VACUUM;"
sqlite3 /var/lib/harbor-cache/harbor-cache.db "ANALYZE;"
```

Consider running optimization during low-traffic periods.

## Capacity Planning

### Estimating Cache Size

**Formula:**
```
Cache Size = (Active Images) x (Average Image Size) x (1.5 safety margin)
```

**Example calculations:**

| Scenario | Active Images | Avg Size | Recommended Cache |
|----------|--------------|----------|-------------------|
| Small team | 50 | 500 MB | 50 GB |
| Medium org | 200 | 1 GB | 300 GB |
| Large enterprise | 1000 | 2 GB | 3 TB |

### Storage Considerations

**Local Storage:**
- SSD/NVMe for performance
- RAID for redundancy (optional, cache is rebuildable)
- Separate volume from OS

**S3 Storage:**
- Standard tier for frequently accessed cache
- Consider Intelligent-Tiering for variable access patterns
- Cross-region replication for DR

### Network Bandwidth

**Formula:**
```
Bandwidth = (Pull Rate) x (Average Image Size) / (Time Window)
```

**Example:**
- 100 pulls/hour
- 500 MB average image
- Peak bandwidth: ~14 MB/s (112 Mbps)

Add 50% headroom for cache misses (upstream fetches).

### Scaling Guidelines

| Metric | Action |
|--------|--------|
| Hit rate < 60% | Increase cache size or retention |
| Response time > 500ms | Check disk I/O, consider SSD |
| Memory usage > 80% | Increase memory allocation |
| CPU usage > 70% sustained | Add more instances |

## High Availability

### Single Instance with Shared Storage

For most deployments, a single instance with S3 storage provides adequate availability:

```
┌──────────────┐     ┌──────────────┐
│ Harbor Cache │────▶│     S3       │
│   Instance   │     │   Bucket     │
└──────────────┘     └──────────────┘
```

Recovery: Launch new instance pointing to same S3 bucket.

### Multi-Instance Setup

For zero-downtime deployments:

```
                    ┌──────────────────┐
                    │   Load Balancer  │
                    └────────┬─────────┘
                             │
              ┌──────────────┼──────────────┐
              │              │              │
       ┌──────▼──────┐ ┌─────▼─────┐ ┌──────▼──────┐
       │  Instance 1 │ │Instance 2 │ │ Instance 3  │
       └──────┬──────┘ └─────┬─────┘ └──────┬──────┘
              │              │              │
              └──────────────┼──────────────┘
                             │
                    ┌────────▼────────┐
                    │    S3 Bucket    │
                    └─────────────────┘
```

**Configuration:**

```toml
[storage]
backend = "s3"

[storage.s3]
bucket = "harbor-cache-shared"
region = "us-west-2"
```

**Load Balancer Configuration:**
- Health check: `GET /health`
- Session affinity: Not required
- Drain timeout: 60 seconds

### Kubernetes Deployment

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: harbor-cache
spec:
  replicas: 3
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
        resources:
          requests:
            memory: "512Mi"
            cpu: "250m"
          limits:
            memory: "2Gi"
            cpu: "2000m"
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
---
apiVersion: policy/v1
kind: PodDisruptionBudget
metadata:
  name: harbor-cache-pdb
spec:
  minAvailable: 2
  selector:
    matchLabels:
      app: harbor-cache
```

## Backup and Recovery

### What to Backup

| Component | Importance | Recovery Impact |
|-----------|------------|-----------------|
| Database | Critical | Lose metadata, stats, users |
| Configuration | Critical | Need to reconfigure |
| Cache data | Optional | Rebuild from upstream |

### Database Backup

**Automated backup script:**

```bash
#!/bin/bash
# /usr/local/bin/backup-harbor-cache.sh

BACKUP_DIR="/backup/harbor-cache"
DB_PATH="/var/lib/harbor-cache/harbor-cache.db"
DATE=$(date +%Y%m%d-%H%M%S)

# Create backup directory
mkdir -p "$BACKUP_DIR"

# Backup database with SQLite backup command
sqlite3 "$DB_PATH" ".backup '$BACKUP_DIR/harbor-cache-$DATE.db'"

# Compress
gzip "$BACKUP_DIR/harbor-cache-$DATE.db"

# Keep last 7 days
find "$BACKUP_DIR" -name "*.db.gz" -mtime +7 -delete

echo "Backup completed: $BACKUP_DIR/harbor-cache-$DATE.db.gz"
```

**Cron schedule:**

```cron
# Daily backup at 2 AM
0 2 * * * /usr/local/bin/backup-harbor-cache.sh >> /var/log/harbor-cache-backup.log 2>&1
```

### Configuration Backup

```bash
# Backup configuration
cp /etc/harbor-cache/config.toml /backup/harbor-cache/config-$(date +%Y%m%d).toml

# Include in version control (redact secrets)
```

### Recovery Procedures

**Database Recovery:**

```bash
# Stop Harbor Cache
systemctl stop harbor-cache

# Restore database
gunzip -c /backup/harbor-cache/harbor-cache-20240115-020000.db.gz > /var/lib/harbor-cache/harbor-cache.db

# Set permissions
chown harbor-cache:harbor-cache /var/lib/harbor-cache/harbor-cache.db

# Start Harbor Cache
systemctl start harbor-cache
```

**Full Recovery:**

1. Deploy new instance
2. Apply configuration
3. Restore database
4. Verify health
5. Update DNS/load balancer

## Monitoring and Alerting

### Key Metrics

**Prometheus queries for essential metrics:**

```yaml
# Cache hit rate
rate(harbor_cache_hits_total[5m]) / (rate(harbor_cache_hits_total[5m]) + rate(harbor_cache_misses_total[5m]))

# Request rate
rate(harbor_cache_requests_total[5m])

# Cache size
harbor_cache_size_bytes

# Entry count
harbor_cache_entries
```

### Alerting Rules

```yaml
groups:
- name: harbor-cache-alerts
  rules:
  # Instance down
  - alert: HarborCacheDown
    expr: up{job="harbor-cache"} == 0
    for: 1m
    labels:
      severity: critical
    annotations:
      summary: "Harbor Cache instance is down"
      description: "{{ $labels.instance }} has been down for more than 1 minute."

  # Low hit rate
  - alert: HarborCacheLowHitRate
    expr: |
      rate(harbor_cache_hits_total[1h]) /
      (rate(harbor_cache_hits_total[1h]) + rate(harbor_cache_misses_total[1h])) < 0.5
    for: 30m
    labels:
      severity: warning
    annotations:
      summary: "Harbor Cache hit rate is low"
      description: "Cache hit rate is {{ $value | humanizePercentage }}, below 50%"

  # Cache nearly full
  - alert: HarborCacheNearlyFull
    expr: harbor_cache_size_bytes / harbor_cache_max_size_bytes > 0.9
    for: 10m
    labels:
      severity: warning
    annotations:
      summary: "Harbor Cache is nearly full"
      description: "Cache is {{ $value | humanizePercentage }} full"

  # High error rate
  - alert: HarborCacheHighErrorRate
    expr: |
      rate(harbor_cache_errors_total[5m]) / rate(harbor_cache_requests_total[5m]) > 0.05
    for: 5m
    labels:
      severity: warning
    annotations:
      summary: "Harbor Cache error rate is high"
      description: "Error rate is {{ $value | humanizePercentage }}"
```

### Grafana Dashboard

**Key panels to include:**

1. **Overview Row:**
   - Health status
   - Uptime
   - Version

2. **Cache Performance Row:**
   - Hit rate gauge (target: >70%)
   - Hits/Misses over time
   - Request rate

3. **Storage Row:**
   - Cache size
   - Entry count
   - Size over time

4. **System Row:**
   - Memory usage
   - CPU usage
   - Network I/O

### Health Checks

**HTTP health check:**

```bash
curl -f http://localhost:5001/health
```

Expected response:

```json
{"status": "healthy", "version": "0.1.0"}
```

**Deep health check script:**

```bash
#!/bin/bash
# Check basic health
if ! curl -sf http://localhost:5001/health > /dev/null; then
    echo "CRITICAL: Health endpoint not responding"
    exit 2
fi

# Check API functionality
if ! curl -sf http://localhost:5001/v2/ > /dev/null; then
    echo "CRITICAL: Registry API not responding"
    exit 2
fi

# Check metrics endpoint
if ! curl -sf http://localhost:5001/metrics > /dev/null; then
    echo "WARNING: Metrics endpoint not responding"
    exit 1
fi

echo "OK: All health checks passed"
exit 0
```

## Log Analysis

### Log Configuration

**Production logging:**

```toml
[logging]
level = "info"
format = "json"
```

### Log Levels

| Level | Use Case |
|-------|----------|
| `error` | Production (minimal) |
| `warn` | Production (recommended) |
| `info` | Production (verbose) |
| `debug` | Troubleshooting |
| `trace` | Development only |

### JSON Log Structure

```json
{
  "timestamp": "2024-01-15T10:30:45.123Z",
  "level": "INFO",
  "target": "harbor_api::routes::registry",
  "message": "Manifest request",
  "repository": "library/nginx",
  "reference": "latest",
  "cache_hit": true,
  "duration_ms": 15
}
```

### Log Aggregation

**Fluentd configuration:**

```xml
<source>
  @type tail
  path /var/log/harbor-cache/*.log
  tag harbor-cache
  <parse>
    @type json
    time_key timestamp
    time_format %Y-%m-%dT%H:%M:%S.%LZ
  </parse>
</source>

<filter harbor-cache>
  @type record_transformer
  <record>
    service harbor-cache
    environment production
  </record>
</filter>

<match harbor-cache>
  @type elasticsearch
  host elasticsearch.example.com
  port 9200
  index_name harbor-cache
</match>
```

### Useful Log Queries

**Elasticsearch/Kibana:**

```json
// High latency requests
{
  "query": {
    "bool": {
      "must": [
        { "range": { "duration_ms": { "gte": 1000 } } }
      ]
    }
  }
}

// Cache misses
{
  "query": {
    "bool": {
      "must": [
        { "term": { "cache_hit": false } },
        { "term": { "level": "INFO" } }
      ]
    }
  }
}

// Errors by type
{
  "aggs": {
    "error_types": {
      "terms": { "field": "error_code.keyword" }
    }
  }
}
```

## Maintenance Operations

### Scheduled Maintenance

**Maintenance window checklist:**

1. Notify users of planned maintenance
2. Drain connections (update load balancer)
3. Perform maintenance tasks
4. Verify health
5. Restore traffic
6. Monitor for issues

### Cache Cleanup

**Manual cleanup:**

```bash
# Get token
TOKEN=$(curl -s -X POST http://localhost:5001/api/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"***"}' | jq -r '.token')

# Run cleanup
curl -X POST http://localhost:5001/api/v1/cache/cleanup \
  -H "Authorization: Bearer $TOKEN"
```

**Scheduled cleanup:**

```bash
# /usr/local/bin/harbor-cache-cleanup.sh
#!/bin/bash

TOKEN=$(curl -s -X POST http://localhost:5001/api/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"'"$ADMIN_PASSWORD"'"}' | jq -r '.token')

curl -X POST http://localhost:5001/api/v1/cache/cleanup \
  -H "Authorization: Bearer $TOKEN"
```

```cron
# Weekly cleanup on Sunday at 3 AM
0 3 * * 0 /usr/local/bin/harbor-cache-cleanup.sh >> /var/log/harbor-cache-cleanup.log 2>&1
```

### Database Maintenance

```bash
# Weekly database optimization
0 4 * * 0 sqlite3 /var/lib/harbor-cache/harbor-cache.db "VACUUM; ANALYZE;" 2>&1
```

### Rolling Updates

**Kubernetes:**

```bash
# Update image
kubectl set image deployment/harbor-cache harbor-cache=ghcr.io/lablup/harbor-cache:v1.1.0

# Monitor rollout
kubectl rollout status deployment/harbor-cache

# Rollback if needed
kubectl rollout undo deployment/harbor-cache
```

**Docker Compose:**

```bash
# Pull new image
docker compose pull

# Rolling restart
docker compose up -d --no-deps harbor-cache

# Watch logs
docker compose logs -f harbor-cache
```

## Disaster Recovery

### Recovery Time Objectives

| Scenario | RTO Target | Strategy |
|----------|------------|----------|
| Instance failure | 5 minutes | Auto-restart/reschedule |
| Storage failure | 30 minutes | Restore from backup |
| Region failure | 1 hour | Failover to DR region |
| Full loss | 4 hours | Full rebuild |

### DR Procedures

**Instance Recovery:**

1. New instance launches automatically (K8s/systemd)
2. Connects to existing storage
3. Resumes serving requests

**Storage Recovery (Local):**

1. Provision new storage
2. Restore database from backup
3. Cache rebuilds automatically from upstream

**Storage Recovery (S3):**

1. Enable cross-region replication
2. Update configuration to point to replica bucket
3. Resume operations

**Full Recovery:**

1. Deploy infrastructure (Terraform/CloudFormation)
2. Deploy Harbor Cache
3. Restore database
4. Configure DNS
5. Verify functionality
6. Resume operations

### Testing DR

**Quarterly DR test:**

1. Simulate failure (stop primary instance)
2. Verify automatic recovery
3. Measure actual recovery time
4. Document gaps
5. Update procedures

## Next Steps

- [Security Documentation](../SECURITY.md) - Security hardening
- [Troubleshooting Guide](troubleshooting.md) - Problem resolution
- [Configuration Reference](configuration.md) - All configuration options
