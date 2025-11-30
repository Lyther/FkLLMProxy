# Production Deployment Guide

**Last Updated**: Current Session
**Status**: ✅ Production-Ready (Vertex & Anthropic)

---

## Overview

This guide covers deploying FkLLMProxy to production environments. The system is production-ready for Vertex AI and Anthropic providers with comprehensive monitoring, graceful shutdown, and resilience features.

---

## Pre-Deployment Checklist

### ✅ System Requirements

- **Rust**: 1.70+ (for building from source)
- **Docker**: 20.10+ (for containerized deployment)
- **OS**: Linux (Ubuntu 20.04+, Debian 11+, or Alpine Linux)
- **Memory**: Minimum 512MB RAM, recommended 1GB+
- **Network**: Outbound HTTPS access to:
  - `generativelanguage.googleapis.com` (Google AI Studio)
  - `{region}-aiplatform.googleapis.com` (Vertex AI)
  - `api.anthropic.com` (via bridge)
  - Custom bridge URLs (if configured)

### ✅ Security Checklist

- [ ] All secrets stored as environment variables (not hardcoded)
- [ ] Authentication enabled (`APP_AUTH__REQUIRE_AUTH=true`)
- [ ] Strong master key configured (`APP_AUTH__MASTER_KEY`)
- [ ] Rate limiting configured appropriately
- [ ] Request size limits configured
- [ ] HTTPS/TLS termination configured (via reverse proxy)
- [ ] Firewall rules restrict access
- [ ] Service account credentials secured (600 permissions)

---

## Deployment Options

### Option 1: Docker Compose (Recommended)

Best for: Single-server deployments, development staging, quick production setup.

**Steps**:

1. **Clone and configure**:

```bash
git clone <repo-url>
cd FkLLMProxy
```

2. **Create `.env` file**:

```bash
# Required
GOOGLE_API_KEY=AIzaSy...  # OR use service account
GOOGLE_APPLICATION_CREDENTIALS=/path/to/service-account.json

# Server Configuration
APP_SERVER__HOST=0.0.0.0
APP_SERVER__PORT=4000

# Security
APP_AUTH__REQUIRE_AUTH=true
APP_AUTH__MASTER_KEY=sk-production-key-here

# Logging (JSON for production)
APP_LOG__LEVEL=info
APP_LOG__FORMAT=json

# Rate Limiting
APP_RATE_LIMIT__CAPACITY=1000
APP_RATE_LIMIT__REFILL_PER_SECOND=100

# Request Limits
APP_SERVER__MAX_REQUEST_SIZE=10485760  # 10MB

# Provider Configuration
APP_ANTHROPIC__BRIDGE_URL=http://bridge:4001

# Circuit Breaker
APP_CIRCUIT_BREAKER__FAILURE_THRESHOLD=10
APP_CIRCUIT_BREAKER__TIMEOUT_SECS=60
APP_CIRCUIT_BREAKER__SUCCESS_THRESHOLD=3
```

3. **Start services**:

```bash
docker-compose up -d
```

4. **Verify deployment**:

```bash
curl http://localhost:4000/health
curl -H "Authorization: Bearer sk-production-key-here" \
     http://localhost:4000/v1/chat/completions \
     -d '{"model": "gemini-pro", "messages": [{"role": "user", "content": "test"}]}'
```

---

### Option 2: Docker (Standalone)

Best for: Container orchestration (Kubernetes, Nomad), custom infrastructure.

**Steps**:

1. **Build Docker image**:

```bash
docker build -t fkllmproxy:latest .
```

2. **Run container**:

```bash
docker run -d \
  --name fkllmproxy \
  -p 4000:4000 \
  -e GOOGLE_API_KEY=AIzaSy... \
  -e APP_AUTH__REQUIRE_AUTH=true \
  -e APP_AUTH__MASTER_KEY=sk-production-key \
  -e APP_LOG__FORMAT=json \
  fkllmproxy:latest
```

**With service account**:

```bash
docker run -d \
  --name fkllmproxy \
  -p 4000:4000 \
  -v /path/to/service-account.json:/app/creds.json:ro \
  -e GOOGLE_APPLICATION_CREDENTIALS=/app/creds.json \
  -e APP_VERTEX__PROJECT_ID=your-project-id \
  -e APP_AUTH__REQUIRE_AUTH=true \
  -e APP_AUTH__MASTER_KEY=sk-production-key \
  -e APP_LOG__FORMAT=json \
  fkllmproxy:latest
```

---

### Option 3: Binary Deployment

Best for: Direct server deployment, systemd services, non-containerized environments.

**Steps**:

1. **Build release binary**:

```bash
cargo build --release
```

2. **Install binary**:

```bash
sudo cp target/release/vertex-bridge /usr/local/bin/fkllmproxy
sudo chmod +x /usr/local/bin/fkllmproxy
```

3. **Create systemd service** (`/etc/systemd/system/fkllmproxy.service`):

```ini
[Unit]
Description=FkLLMProxy - LLM Provider Bridge
After=network.target

[Service]
Type=simple
User=fkllmproxy
Group=fkllmproxy
WorkingDirectory=/opt/fkllmproxy
EnvironmentFile=/etc/fkllmproxy/env
ExecStart=/usr/local/bin/fkllmproxy
Restart=always
RestartSec=10
StandardOutput=journal
StandardError=journal

# Security
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/opt/fkllmproxy

# Resource Limits
LimitNOFILE=65536
MemoryLimit=1G

[Install]
WantedBy=multi-user.target
```

4. **Create environment file** (`/etc/fkllmproxy/env`):

```bash
GOOGLE_API_KEY=AIzaSy...
APP_AUTH__REQUIRE_AUTH=true
APP_AUTH__MASTER_KEY=sk-production-key
APP_LOG__FORMAT=json
```

5. **Secure environment file**:

```bash
sudo chmod 600 /etc/fkllmproxy/env
sudo chown fkllmproxy:fkllmproxy /etc/fkllmproxy/env
```

6. **Start service**:

```bash
sudo systemctl daemon-reload
sudo systemctl enable fkllmproxy
sudo systemctl start fkllmproxy
sudo systemctl status fkllmproxy
```

---

## Production Configuration

### Recommended Settings

```bash
# Server
APP_SERVER__HOST=0.0.0.0
APP_SERVER__PORT=4000
APP_SERVER__MAX_REQUEST_SIZE=10485760  # 10MB

# Security (REQUIRED in production)
APP_AUTH__REQUIRE_AUTH=true
APP_AUTH__MASTER_KEY=<strong-random-key>

# Logging (JSON for log aggregation)
APP_LOG__LEVEL=info
APP_LOG__FORMAT=json

# Rate Limiting (adjust based on load)
APP_RATE_LIMIT__CAPACITY=1000
APP_RATE_LIMIT__REFILL_PER_SECOND=100

# Circuit Breaker
APP_CIRCUIT_BREAKER__FAILURE_THRESHOLD=10
APP_CIRCUIT_BREAKER__TIMEOUT_SECS=60
APP_CIRCUIT_BREAKER__SUCCESS_THRESHOLD=3

# Vertex AI (if using service account)
APP_VERTEX__PROJECT_ID=your-project-id
APP_VERTEX__REGION=us-central1

# Anthropic Bridge
APP_ANTHROPIC__BRIDGE_URL=http://bridge:4001
```

### Security Best Practices

1. **Enable Authentication**:

```bash
APP_AUTH__REQUIRE_AUTH=true
APP_AUTH__MASTER_KEY=$(openssl rand -hex 32)
```

2. **Use HTTPS**: Deploy behind a reverse proxy (nginx, Traefik, Caddy) with TLS:

```nginx
server {
    listen 443 ssl http2;
    server_name api.example.com;

    ssl_certificate /etc/ssl/certs/api.crt;
    ssl_certificate_key /etc/ssl/private/api.key;

    location / {
        proxy_pass http://127.0.0.1:4000;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}
```

3. **Firewall Rules**: Restrict access to trusted sources:

```bash
# Allow only specific IPs
sudo ufw allow from 10.0.0.0/8 to any port 4000
sudo ufw deny 4000
```

4. **Service Account Security**:

```bash
# Store credentials securely
chmod 600 ~/.config/fkllmproxy/service-account.json
chown fkllmproxy:fkllmproxy ~/.config/fkllmproxy/service-account.json
```

---

## Monitoring & Observability

### Health Checks

The `/health` endpoint provides system health status:

```bash
curl http://localhost:4000/health
```

Response:

```json
{
  "status": "healthy",
  "version": "0.1.0",
  "timestamp": "2024-01-01T00:00:00Z"
}
```

### Metrics

**JSON Metrics** (`/metrics`):

```bash
curl http://localhost:4000/metrics
```

**Prometheus Metrics** (`/metrics/prometheus`):

```bash
curl http://localhost:4000/metrics/prometheus
```

Available metrics:

- `requests_total` - Total requests
- `requests_failed_total` - Failed requests
- `request_success_rate` - Success rate percentage
- `request_latency_ms` - Average latency
- `request_latency_p50_ms` - 50th percentile latency
- `request_latency_p95_ms` - 95th percentile latency
- `request_latency_p99_ms` - 99th percentile latency
- `cache_hits_total` - Cache hits
- `cache_misses_total` - Cache misses
- `cache_hit_rate` - Cache hit rate

### Prometheus Configuration

Example `prometheus.yml`:

```yaml
scrape_configs:
  - job_name: 'fkllmproxy'
    scrape_interval: 15s
    static_configs:
      - targets: ['localhost:4000']
    metrics_path: '/metrics/prometheus'
```

### Logging

**JSON Format** (production):

```bash
APP_LOG__FORMAT=json
```

Example log entry:

```json
{
  "timestamp": "2024-01-01T00:00:00.000Z",
  "level": "INFO",
  "target": "vertex_bridge",
  "fields": {
    "message": "Received request",
    "request_id": "550e8400-e29b-41d4-a716-446655440000",
    "model": "gemini-pro"
  }
}
```

**Pretty Format** (development):

```bash
APP_LOG__FORMAT=pretty
```

---

## Graceful Shutdown

The server handles graceful shutdown on:

- `SIGTERM` (Kubernetes, Docker stop)
- `SIGINT` (Ctrl+C)
- `Ctrl+C` (Windows)

On shutdown:

1. Stop accepting new connections
2. Allow in-flight requests to complete
3. Close connections gracefully
4. Exit cleanly

**Shutdown timeout**: Default 30 seconds (configure via reverse proxy).

---

## Reverse Proxy Setup

### Nginx

```nginx
upstream fkllmproxy {
    server 127.0.0.1:4000;
    keepalive 32;
}

server {
    listen 80;
    server_name api.example.com;

    location / {
        proxy_pass http://fkllmproxy;
        proxy_http_version 1.1;
        proxy_set_header Connection "";
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;

        # Timeouts
        proxy_connect_timeout 60s;
        proxy_send_timeout 300s;
        proxy_read_timeout 300s;

        # Body size
        client_max_body_size 10M;
    }
}
```

### Caddy

```caddy
api.example.com {
    reverse_proxy localhost:4000 {
        header_up X-Real-IP {remote_host}
        header_up X-Forwarded-For {remote_host}
        header_up X-Forwarded-Proto {scheme}
    }
}
```

---

## Scaling & High Availability

### Horizontal Scaling

Run multiple instances behind a load balancer:

```bash
# Instance 1
docker run -d --name fkllmproxy-1 -p 4001:4000 fkllmproxy:latest

# Instance 2
docker run -d --name fkllmproxy-2 -p 4002:4000 fkllmproxy:latest
```

Load balancer configuration:

```nginx
upstream fkllmproxy {
    least_conn;
    server 127.0.0.1:4001;
    server 127.0.0.1:4002;
    keepalive 32;
}
```

### Resource Limits

**Docker**:

```yaml
services:
  proxy:
    deploy:
      resources:
        limits:
          cpus: '2'
          memory: 1G
        reservations:
          cpus: '0.5'
          memory: 512M
```

**systemd**:

```ini
[Service]
MemoryLimit=1G
CPUQuota=200%
```

---

## Troubleshooting

### Common Issues

1. **Connection Refused**:

```bash
# Check if service is running
sudo systemctl status fkllmproxy

# Check logs
sudo journalctl -u fkllmproxy -f
```

2. **Authentication Failures**:

```bash
# Verify master key is set
echo $APP_AUTH__MASTER_KEY

# Test authentication
curl -H "Authorization: Bearer $APP_AUTH__MASTER_KEY" \
     http://localhost:4000/health
```

3. **Rate Limiting**:

```bash
# Check rate limit headers
curl -v http://localhost:4000/v1/chat/completions \
     -H "Authorization: Bearer $APP_AUTH__MASTER_KEY"

# Adjust limits in config
APP_RATE_LIMIT__CAPACITY=2000
APP_RATE_LIMIT__REFILL_PER_SECOND=200
```

4. **Provider Errors**:

```bash
# Check provider connectivity
curl http://localhost:4000/health

# Verify credentials
echo $GOOGLE_API_KEY
ls -la $GOOGLE_APPLICATION_CREDENTIALS
```

---

## Backup & Recovery

### Configuration Backup

```bash
# Backup environment files
tar -czf fkllmproxy-config-$(date +%Y%m%d).tar.gz \
    /etc/fkllmproxy/env \
    /etc/systemd/system/fkllmproxy.service
```

### Rollback Procedure

1. Stop current service:

```bash
sudo systemctl stop fkllmproxy
```

2. Restore previous version:

```bash
sudo cp /backup/fkllmproxy-previous /usr/local/bin/fkllmproxy
```

3. Restore configuration:

```bash
sudo cp /backup/env /etc/fkllmproxy/env
```

4. Restart service:

```bash
sudo systemctl start fkllmproxy
```

---

## Performance Tuning

### Recommended Settings

**High Traffic**:

```bash
APP_RATE_LIMIT__CAPACITY=5000
APP_RATE_LIMIT__REFILL_PER_SECOND=500
APP_SERVER__MAX_REQUEST_SIZE=52428800  # 50MB
```

**Low Latency**:

```bash
APP_RATE_LIMIT__CAPACITY=2000
APP_RATE_LIMIT__REFILL_PER_SECOND=200
```

**High Concurrency**:

```bash
# Increase file descriptors
ulimit -n 65536

# systemd
[Service]
LimitNOFILE=65536
```

---

## Next Steps

- See [Operational Runbook](runbook.md) for day-to-day operations
- See [Architecture](architecture.md) for system design
- See [IMPLEMENTATION_STATUS.md](IMPLEMENTATION_STATUS.md) for feature status

---

**Status**: ✅ Production-Ready for Vertex & Anthropic
