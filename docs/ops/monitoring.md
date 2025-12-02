# Monitoring & Observability Guide

This guide covers setting up monitoring, metrics collection, and observability for FkLLMProxy in production.

## Metrics Endpoints

FkLLMProxy exposes metrics in two formats:

### JSON Metrics (`/metrics`)

Returns JSON with current metrics:

```bash
curl http://localhost:4000/metrics
```

Response includes:

- `cache_hit_rate`: Token cache hit percentage
- `waf_block_rate`: WAF block percentage
- `arkose_solves`: Number of Arkose tokens generated
- `avg_arkose_solve_time_ms`: Average Arkose solve time
- `total_requests`: Total requests processed
- `success_rate`: Request success percentage

### Prometheus Metrics (`/metrics/prometheus`)

Returns Prometheus-formatted metrics for scraping:

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
- `waf_blocks_total` - WAF blocks
- `waf_block_rate` - WAF block rate
- `arkose_solves_total` - Arkose solves
- `arkose_solve_time_ms` - Average solve time

## Prometheus Setup

### Configuration

Add to `prometheus.yml`:

```yaml
scrape_configs:
  - job_name: 'fkllmproxy'
    scrape_interval: 15s
    static_configs:
      - targets: ['localhost:4000']
    metrics_path: '/metrics/prometheus'
```

### Kubernetes ServiceMonitor

If using Prometheus Operator:

```yaml
apiVersion: monitoring.coreos.com/v1
kind: ServiceMonitor
metadata:
  name: fkllmproxy
  namespace: default
spec:
  selector:
    matchLabels:
      app: fkllmproxy
  endpoints:
  - port: http
    path: /metrics/prometheus
    interval: 15s
```

## Grafana Dashboards

### Basic Dashboard

Create a dashboard with:

1. **Request Rate**: `rate(requests_total[5m])`
2. **Success Rate**: `request_success_rate`
3. **Error Rate**: `rate(requests_failed_total[5m])`
4. **Latency (p95)**: `request_latency_p95_ms`
5. **WAF Blocks**: `rate(waf_blocks_total[5m])`
6. **Cache Hit Rate**: `cache_hit_rate`

### Example Queries

**Request Rate**:

```promql
sum(rate(requests_total[5m])) by (instance)
```

**Error Rate**:

```promql
sum(rate(requests_failed_total[5m])) / sum(rate(requests_total[5m]))
```

**P95 Latency**:

```promql
request_latency_p95_ms
```

## Alerting Rules

### Example Prometheus Alert Rules

```yaml
groups:
  - name: fkllmproxy
    rules:
      - alert: HighErrorRate
        expr: rate(requests_failed_total[5m]) / rate(requests_total[5m]) > 0.1
        for: 5m
        annotations:
          summary: "High error rate detected"
          description: "Error rate is {{ $value | humanizePercentage }}"

      - alert: HighLatency
        expr: request_latency_p95_ms > 5000
        for: 5m
        annotations:
          summary: "High latency detected"
          description: "P95 latency is {{ $value }}ms"

      - alert: WAFBlocking
        expr: rate(waf_blocks_total[5m]) > 0
        for: 1m
        annotations:
          summary: "WAF blocking requests"
          description: "{{ $value }} WAF blocks in the last 5 minutes"

      - alert: ServiceDown
        expr: up{job="fkllmproxy"} == 0
        for: 1m
        annotations:
          summary: "FkLLMProxy service is down"
```

## Health Checks

The `/health` endpoint provides service health status:

```bash
curl http://localhost:4000/health
```

Response includes:

- Overall status
- Harvester connectivity
- Anthropic bridge connectivity
- Timestamp

Use this for:

- Kubernetes liveness/readiness probes
- Load balancer health checks
- Monitoring system checks

## Logging

### Structured JSON Logging

Enable JSON logging for production:

```bash
APP_LOG__FORMAT=json
```

Logs include:

- Timestamp
- Level
- Message
- Context fields (request_id, model, etc.)
- File and line number

### Log Aggregation

Recommended tools:

- **Loki**: Prometheus-compatible log aggregation
- **ELK Stack**: Elasticsearch, Logstash, Kibana
- **Datadog**: Commercial log management
- **CloudWatch**: AWS-native logging

### Example Log Queries

**Error logs**:

```json
{ "level": "error" }
```

**OpenAI requests**:

```json
{ "model": "gpt-4" }
```

**WAF blocks**:

```json
{ "status": 403 }
```

## Distributed Tracing

> **Note**: OpenTelemetry integration is planned. See [Enhanced Monitoring](../dev/architecture/system-overview.md) for details.

When implemented, traces will include:

- Request ID propagation
- Provider call spans
- Circuit breaker state
- Token acquisition time

## Performance Monitoring

### Key Metrics to Track

1. **Request Throughput**: Requests per second
2. **Latency Percentiles**: p50, p95, p99
3. **Error Rates**: By status code and provider
4. **Circuit Breaker State**: Open/closed transitions
5. **Cache Performance**: Hit/miss rates
6. **Resource Usage**: CPU, memory per service

### Benchmarking

See [Performance Testing Guide](../dev/testing/guide.md) for load testing procedures.

## Troubleshooting

### High Error Rates

1. Check provider health: `curl /health`
2. Review error logs: Filter by `level: error`
3. Check circuit breaker state in metrics
4. Verify credentials are valid

### High Latency

1. Check provider-specific latency
2. Review cache hit rates
3. Check network connectivity
4. Monitor resource usage (CPU/memory)

### WAF Blocks

1. Check token freshness
2. Review request patterns
3. See [TLS Fingerprinting ADR](../dev/adr/005-tls-fingerprinting.md) for future implementation

## Best Practices

1. **Scrape Interval**: Use 15-30s for production
2. **Retention**: Keep metrics for 30-90 days
3. **Alerting**: Set thresholds based on SLOs
4. **Dashboards**: Create separate dashboards per environment
5. **Log Levels**: Use `info` for production, `debug` for troubleshooting

## References

- [Prometheus Documentation](https://prometheus.io/docs/)
- [Grafana Dashboards](https://grafana.com/docs/grafana/latest/dashboards/)
- [OpenTelemetry](https://opentelemetry.io/) (planned)
