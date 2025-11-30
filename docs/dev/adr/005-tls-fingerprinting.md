# ADR 005: TLS Fingerprinting for OpenAI WAF Bypass

**Status**: Accepted (Partial Implementation)
**Date**: 2025-11-30
**Deciders**: Architecture Team

## Context

OpenAI's ChatGPT web interface is protected by Cloudflare WAF, which uses TLS fingerprinting (JA3/JA4) to detect and block automated requests. Standard HTTP clients like `reqwest` with default TLS settings produce fingerprints that differ from real browsers, causing requests to be blocked with 403 errors.

## Decision

We will implement TLS fingerprinting support to bypass Cloudflare WAF by matching browser TLS signatures. The implementation will be configurable and support multiple browser targets.

### Implementation Approach

1. **Configuration**: Added `tls_fingerprint_enabled` and `tls_fingerprint_target` to `OpenAIConfig`
2. **Client Enhancement**: Updated `OpenAIBackendClient` to support TLS fingerprinting configuration
3. **Future Implementation**: Full TLS fingerprinting requires `reqwest-impersonate` or similar library

### Current Status

- ✅ Configuration structure in place
- ✅ Client updated to read TLS fingerprinting config
- ⚠️ Full TLS fingerprinting not yet implemented (requires external library)

## Consequences

### Positive

- **Configurable**: Can enable/disable TLS fingerprinting per deployment
- **Extensible**: Structure supports multiple browser targets (Chrome, Firefox, etc.)
- **Backward Compatible**: Defaults to disabled, existing deployments unaffected

### Negative

- **Incomplete**: Full implementation requires `reqwest-impersonate` which has dependencies:
  - Requires BoringSSL fork
  - May have build complexity
  - Performance overhead
- **Maintenance**: TLS fingerprints change with browser updates
- **WAF Evolution**: Cloudflare may update detection methods

## Implementation Options

### Option A: reqwest-impersonate (Recommended)

**Pros**:

- Actively maintained
- Supports multiple browser fingerprints
- Good documentation

**Cons**:

- Requires BoringSSL fork
- Build complexity
- Larger binary size

**Status**: Research phase - evaluating integration approach

### Option B: curl-impersonate via FFI

**Pros**:

- Mature implementation
- Battle-tested

**Cons**:

- FFI overhead
- External dependency
- Less Rust-native

**Status**: Not evaluated

### Option C: Custom TLS Configuration

**Pros**:

- Full control
- No external dependencies

**Cons**:

- Complex to implement correctly
- Requires deep TLS knowledge
- Maintenance burden

**Status**: Not recommended

## Configuration

```bash
# Enable TLS fingerprinting
APP_OPENAI__TLS_FINGERPRINT_ENABLED=true
APP_OPENAI__TLS_FINGERPRINT_TARGET=chrome120
```

Supported targets:

- `chrome120` - Chrome 120 TLS fingerprint
- `firefox120` - Firefox 120 TLS fingerprint

## Next Steps

1. **Research Phase** (4 hours):
   - Evaluate `reqwest-impersonate` integration
   - Test build process
   - Benchmark performance impact

2. **Implementation Phase** (4-6 hours):
   - Add `reqwest-impersonate` dependency
   - Implement fingerprint matching
   - Add tests
   - Update documentation

3. **Testing Phase** (1-2 hours):
   - End-to-end testing with OpenAI
   - WAF bypass validation
   - Performance testing

## References

- [reqwest-impersonate](https://github.com/lutzenfried/reqwest-impersonate) - TLS fingerprinting library
- [JA3/JA4 Fingerprinting](https://github.com/salesforce/ja3) - TLS fingerprinting standard
- [Cloudflare WAF](https://developers.cloudflare.com/waf/) - WAF documentation
