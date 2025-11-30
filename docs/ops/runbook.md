# Operational Runbook

**Last Updated**: Current Session
**Status**: ✅ Production-Ready

---

## Quick Reference

### Service Management

```bash
# Check status
sudo systemctl status fkllmproxy

# View logs
sudo journalctl -u fkllmproxy -f

# Restart service
sudo systemctl restart fkllmproxy

# Stop service
sudo systemctl stop fkllmproxy

# Start service
sudo systemctl start fkllmproxy
```

### Health Checks

```bash
# Basic health check
curl http://localhost:4000/health

# With authentication
curl -H "Authorization: Bearer $APP_AUTH__MASTER_KEY" \
     http://localhost:4000/health
```

### Metrics

```bash
# JSON metrics
curl http://localhost:4000/metrics

# Prometheus metrics
curl http://localhost:4000/metrics/prometheus
```

---

## Common Operations

### Viewing Logs

**Live logs** (JSON format):

```bash
sudo journalctl -u fkllmproxy -f | jq
```

**Recent errors**:

```bash
sudo journalctl -u fkllmproxy --since "1 hour ago" | grep ERROR
```

**Filter by level**:

```bash
sudo journalctl -u fkllmproxy -f | jq 'select(.level == "ERROR")'
```

**Search logs**:

```bash
sudo journalctl -u fkllmproxy --since "today" | grep "request_id"
```

### Monitoring Metrics

**Check request rate**:

```bash
watch -n 1 'curl -s http://localhost:4000/metrics/prometheus | grep requests_total'
```

**Check latency**:

```bash
curl -s http://localhost:4000/metrics/prometheus | grep latency
```

**Check error rate**:

```bash
curl -s http://localhost:4000/metrics/prometheus | grep failed
```

### Testing Endpoints

**Health check**:

```bash
curl -v http://localhost:4000/health
```

**Chat completion**:

```bash
curl -X POST http://localhost:4000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $APP_AUTH__MASTER_KEY" \
  -d '{
    "model": "gemini-pro",
    "messages": [{"role": "user", "content": "Hello"}]
  }'
```

---

## Troubleshooting

### Service Won't Start

**Check configuration**:

```bash
# Validate config
/usr/local/bin/fkllmproxy 2>&1 | head -20

# Check environment file
sudo cat /etc/fkllmproxy/env | grep -v "KEY\|PASSWORD"
```

**Common issues**:

1. **Missing credentials**:

   ```bash
   # Check if API key is set
   grep GOOGLE_API_KEY /etc/fkllmproxy/env

   # Check if service account exists
   ls -la $GOOGLE_APPLICATION_CREDENTIALS
   ```

2. **Port already in use**:

   ```bash
   # Check what's using the port
   sudo lsof -i :4000

   # Kill process if needed
   sudo kill -9 <PID>
   ```

3. **Permission issues**:

   ```bash
   # Check file permissions
   ls -la /usr/local/bin/fkllmproxy
   ls -la /etc/fkllmproxy/env

   # Fix if needed
   sudo chmod +x /usr/local/bin/fkllmproxy
   sudo chmod 600 /etc/fkllmproxy/env
   ```

### High Error Rate

**Check error logs**:

```bash
sudo journalctl -u fkllmproxy --since "10 minutes ago" | grep -i error
```

**Check metrics**:

```bash
curl -s http://localhost:4000/metrics | jq '.failed_requests, .success_rate'
```

**Common causes**:

1. **Provider API issues**:

   ```bash
   # Test provider connectivity
   curl -v https://generativelanguage.googleapis.com/v1/models
   ```

2. **Rate limiting**:

   ```bash
   # Check rate limit headers
   curl -v -H "Authorization: Bearer $APP_AUTH__MASTER_KEY" \
        http://localhost:4000/v1/chat/completions \
        -d '{"model": "gemini-pro", "messages": []}' \
        2>&1 | grep -i "rate"
   ```

3. **Circuit breaker open**:

   ```bash
   # Check circuit breaker state in logs
   sudo journalctl -u fkllmproxy | grep -i "circuit"
   ```

### High Latency

**Check latency metrics**:

```bash
curl -s http://localhost:4000/metrics/prometheus | grep latency
```

**Common causes**:

1. **Provider latency**:

   ```bash
   # Test provider directly
   time curl https://generativelanguage.googleapis.com/v1/models
   ```

2. **Resource constraints**:

   ```bash
   # Check CPU/Memory usage
   top -p $(pgrep fkllmproxy)

   # Check system load
   uptime
   ```

3. **Network issues**:

   ```bash
   # Test connectivity
   ping generativelanguage.googleapis.com
   traceroute generativelanguage.googleapis.com
   ```

### Authentication Failures

**Verify master key**:

```bash
# Check if key is set
echo $APP_AUTH__MASTER_KEY

# Test authentication
curl -H "Authorization: Bearer $APP_AUTH__MASTER_KEY" \
     http://localhost:4000/health
```

**Check auth middleware logs**:

```bash
sudo journalctl -u fkllmproxy | grep -i "unauthorized\|auth"
```

---

## Maintenance

### Updating Configuration

1. **Edit environment file**:

```bash
sudo nano /etc/fkllmproxy/env
```

2. **Validate changes**:

```bash
# Check syntax (no validation tool, but check manually)
cat /etc/fkllmproxy/env
```

3. **Restart service**:

```bash
sudo systemctl restart fkllmproxy
sudo systemctl status fkllmproxy
```

### Updating Binary

1. **Stop service**:

```bash
sudo systemctl stop fkllmproxy
```

2. **Backup current binary**:

```bash
sudo cp /usr/local/bin/fkllmproxy /usr/local/bin/fkllmproxy.backup
```

3. **Copy new binary**:

```bash
sudo cp target/release/vertex-bridge /usr/local/bin/fkllmproxy
sudo chmod +x /usr/local/bin/fkllmproxy
```

4. **Start service**:

```bash
sudo systemctl start fkllmproxy
sudo systemctl status fkllmproxy
```

5. **Rollback if needed**:

```bash
sudo systemctl stop fkllmproxy
sudo cp /usr/local/bin/fkllmproxy.backup /usr/local/bin/fkllmproxy
sudo systemctl start fkllmproxy
```

### Rotating Secrets

**Rotate master key**:

1. **Generate new key**:

```bash
NEW_KEY=$(openssl rand -hex 32)
echo $NEW_KEY
```

2. **Update environment file**:

```bash
sudo sed -i "s/APP_AUTH__MASTER_KEY=.*/APP_AUTH__MASTER_KEY=$NEW_KEY/" \
    /etc/fkllmproxy/env
```

3. **Restart service**:

```bash
sudo systemctl restart fkllmproxy
```

4. **Update clients**: Update all client applications with new key.

---

## Emergency Procedures

### Service Crash

1. **Check crash logs**:

```bash
sudo journalctl -u fkllmproxy --since "5 minutes ago" | tail -50
```

2. **Check system resources**:

```bash
df -h
free -h
dmesg | tail -20
```

3. **Restart service**:

```bash
sudo systemctl restart fkllmproxy
```

4. **If restart fails**: Check configuration and rollback if needed.

### High Error Rate

1. **Immediate action**: Check provider status:

```bash
curl -v https://generativelanguage.googleapis.com/v1/models
```

2. **Check circuit breaker**:

```bash
sudo journalctl -u fkllmproxy | grep -i circuit
```

3. **Temporary fix**: Increase circuit breaker thresholds if provider is flaky:

```bash
# Edit config
sudo nano /etc/fkllmproxy/env
# Increase APP_CIRCUIT_BREAKER__FAILURE_THRESHOLD

# Restart
sudo systemctl restart fkllmproxy
```

### Security Incident

1. **Immediately rotate keys**:

```bash
# Generate new master key
NEW_KEY=$(openssl rand -hex 32)

# Update config
sudo sed -i "s/APP_AUTH__MASTER_KEY=.*/APP_AUTH__MASTER_KEY=$NEW_KEY/" \
    /etc/fkllmproxy/env

# Restart service
sudo systemctl restart fkllmproxy
```

2. **Check access logs**:

```bash
sudo journalctl -u fkllmproxy --since "24 hours ago" | grep -i "unauthorized\|auth"
```

3. **Review firewall rules**:

```bash
sudo ufw status
sudo iptables -L -n
```

4. **Notify team**: Alert security team and review incident.

---

## Performance Tuning

### Adjusting Rate Limits

**Increase for high traffic**:

```bash
# Edit config
sudo nano /etc/fkllmproxy/env

# Add/update:
APP_RATE_LIMIT__CAPACITY=5000
APP_RATE_LIMIT__REFILL_PER_SECOND=500

# Restart
sudo systemctl restart fkllmproxy
```

**Decrease for cost control**:

```bash
APP_RATE_LIMIT__CAPACITY=100
APP_RATE_LIMIT__REFILL_PER_SECOND=10
```

### Adjusting Request Size Limits

**Increase for large prompts**:

```bash
APP_SERVER__MAX_REQUEST_SIZE=52428800  # 50MB
```

**Decrease for security**:

```bash
APP_SERVER__MAX_REQUEST_SIZE=5242880  # 5MB
```

### Adjusting Circuit Breaker

**More sensitive** (opens faster):

```bash
APP_CIRCUIT_BREAKER__FAILURE_THRESHOLD=5
APP_CIRCUIT_BREAKER__TIMEOUT_SECS=30
```

**Less sensitive** (more resilient):

```bash
APP_CIRCUIT_BREAKER__FAILURE_THRESHOLD=20
APP_CIRCUIT_BREAKER__TIMEOUT_SECS=120
```

---

## Monitoring Alerts

### Recommended Alerts

1. **Service Down**:
   - Condition: Health check fails for 1 minute
   - Action: Page on-call engineer

2. **High Error Rate**:
   - Condition: Error rate > 10% for 5 minutes
   - Action: Alert team

3. **High Latency**:
   - Condition: P95 latency > 5 seconds for 5 minutes
   - Action: Alert team

4. **Circuit Breaker Open**:
   - Condition: Circuit breaker opens
   - Action: Alert team, check provider status

5. **Rate Limit Hit**:
   - Condition: Rate limit exceeded > 100 times/minute
   - Action: Review rate limit configuration

---

## Useful Commands

### Quick Diagnostics

```bash
# Full system check
echo "=== Service Status ==="
sudo systemctl status fkllmproxy --no-pager

echo "=== Health Check ==="
curl -s http://localhost:4000/health | jq

echo "=== Metrics ==="
curl -s http://localhost:4000/metrics | jq '.total_requests, .success_rate'

echo "=== Recent Errors ==="
sudo journalctl -u fkllmproxy --since "10 minutes ago" | grep -i error | tail -5

echo "=== Resource Usage ==="
ps aux | grep fkllmproxy | grep -v grep
```

### Performance Analysis

```bash
# Request rate
watch -n 1 'curl -s http://localhost:4000/metrics | jq .total_requests'

# Success rate
watch -n 1 'curl -s http://localhost:4000/metrics | jq .success_rate'

# Latency
watch -n 1 'curl -s http://localhost:4000/metrics/prometheus | grep latency'
```

---

## Contact & Escalation

- **On-Call Engineer**: [Contact Info]
- **Team Lead**: [Contact Info]
- **Security Team**: [Contact Info]

**Escalation Path**:

1. Check runbook procedures
2. Check recent changes/deployments
3. Contact on-call engineer
4. Escalate to team lead if unresolved in 30 minutes
5. Escalate to security team for security incidents

---

**Status**: ✅ Production-Ready
