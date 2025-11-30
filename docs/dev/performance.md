# Performance Testing & Optimization Guide

This guide covers performance testing, benchmarking, and optimization for FkLLMProxy.

## Load Testing

### Quick Test

```bash
./scripts/load-test.sh http://localhost:4000 sk-vertex-bridge-dev 10 30s
```

### Using hey

```bash
hey -n 1000 -c 10 -z 30s \
  -H "Authorization: Bearer sk-vertex-bridge-dev" \
  -H "Content-Type: application/json" \
  -d '{"model":"gemini-2.5-flash","messages":[{"role":"user","content":"Hello"}],"max_tokens":10}' \
  -m POST \
  http://localhost:4000/v1/chat/completions
```

### Using wrk

```bash
wrk -t4 -c10 -d30s \
  -H "Authorization: Bearer sk-vertex-bridge-dev" \
  -H "Content-Type: application/json" \
  -s scripts/load-test-lua.lua \
  http://localhost:4000/v1/chat/completions
```

## Performance Benchmarks

### Target Metrics

- **Latency (p95)**: < 2s for non-streaming, < 100ms first chunk for streaming
- **Throughput**: > 100 req/s per instance
- **Memory**: < 512MB per instance
- **CPU**: < 50% under normal load

### Baseline Measurements

Run baseline tests before optimization:

```bash
# Warm up
curl -X POST http://localhost:4000/v1/chat/completions \
  -H "Authorization: Bearer sk-vertex-bridge-dev" \
  -H "Content-Type: application/json" \
  -d '{"model":"gemini-2.5-flash","messages":[{"role":"user","content":"Hello"}]}'

# Measure
time curl -X POST http://localhost:4000/v1/chat/completions \
  -H "Authorization: Bearer sk-vertex-bridge-dev" \
  -H "Content-Type: application/json" \
  -d '{"model":"gemini-2.5-flash","messages":[{"role":"user","content":"Hello"}]}'
```

## Monitoring During Tests

### Prometheus Metrics

```bash
# Watch request rate
watch -n 1 'curl -s http://localhost:4000/metrics/prometheus | grep requests_total'

# Watch latency
watch -n 1 'curl -s http://localhost:4000/metrics/prometheus | grep latency'
```

### System Metrics

```bash
# CPU and Memory
top -p $(pgrep vertex-bridge)

# Or with htop
htop -p $(pgrep vertex-bridge)
```

## Profiling

### CPU Profiling

```bash
# Install perf
sudo apt-get install linux-perf

# Profile
sudo perf record -p $(pgrep vertex-bridge) -g sleep 30
sudo perf report
```

### Memory Profiling

```bash
# Install valgrind
sudo apt-get install valgrind

# Profile
valgrind --tool=massif --massif-out-file=massif.out ./target/release/vertex-bridge
ms_print massif.out
```

## Optimization Tips

### 1. Connection Pooling

Ensure HTTP clients use connection pooling (already implemented with reqwest).

### 2. Response Compression

Compression is enabled by default. Monitor compression ratio:

```bash
curl -H "Accept-Encoding: gzip" -v http://localhost:4000/v1/chat/completions 2>&1 | grep -i "content-encoding"
```

### 3. Caching

Implement response caching for repeated requests (see request caching implementation).

### 4. Streaming

Use streaming for better perceived latency:

```bash
curl -X POST http://localhost:4000/v1/chat/completions \
  -H "Authorization: Bearer sk-vertex-bridge-dev" \
  -H "Content-Type: application/json" \
  -d '{"model":"gemini-2.5-flash","messages":[{"role":"user","content":"Hello"}],"stream":true}'
```

### 5. Provider-Specific Optimization

- **Vertex**: Use API key mode for lower latency
- **Anthropic**: Monitor bridge service latency
- **OpenAI**: TLS fingerprinting may add overhead

## Performance Test Results

### Example Results

**Environment**: 2 vCPU, 4GB RAM, localhost

- **Non-streaming**: p95 latency ~800ms
- **Streaming**: First chunk ~50ms, total ~1.2s
- **Throughput**: ~150 req/s at 10 concurrent
- **Memory**: ~200MB baseline, ~300MB under load

## Troubleshooting

### High Latency

1. Check provider response times
2. Review network connectivity
3. Check for rate limiting
4. Monitor circuit breaker state

### Low Throughput

1. Increase concurrent connections
2. Check CPU usage
3. Review rate limit settings
4. Consider horizontal scaling

### Memory Leaks

1. Run with valgrind
2. Monitor memory over time
3. Check for unbounded collections
4. Review connection pool sizes

## References

- [hey Documentation](https://github.com/rakyll/hey)
- [wrk Documentation](https://github.com/wg/wrk)
- [Rust Performance Book](https://nnethercote.github.io/perf-book/)
