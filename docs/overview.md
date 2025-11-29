# Vertex AI LLM Proxy for Cursor IDE

> **Project Codename:** `vertex-bridge`  
> **Version:** 2.0 (Rust Native)  
> **Last Updated:** November 2025

## Executive Summary

A high-performance, Rust-native LLM proxy that bridges Google Cloud Vertex AI (Gemini) to OpenAI-compatible clients such as Cursor IDE. This enables leveraging Vertex AI credits for AI-assisted development while maintaining full compatibility with tools expecting the OpenAI API format.

---

## Problem Statement

Cursor IDE and similar developer tools are designed around OpenAI's API format. While Google's Vertex AI offers competitive pricing (especially with Gemini Flash models) and generous enterprise credits, there's no native integration path. This project provides a translation layer that:

1. Exposes an OpenAI-compatible `/v1/chat/completions` endpoint
2. Translates requests to Vertex AI's Gemini API format
3. Handles Google Cloud authentication transparently
4. Provides fallback, load balancing, and observability

---

## Architecture Overview

```text
┌─────────────────────────────────────────────────────────────────────────┐
│                              CLIENT LAYER                               │
├─────────────────────────────────────────────────────────────────────────┤
│   Cursor IDE  ←→  Custom Base URL (HTTPS required)                      │
│   Other OpenAI-compatible clients                                       │
└───────────────────────────────────┬─────────────────────────────────────┘
                                    │ HTTPS (TLS 1.3)
                                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                           PROXY LAYER (Rust)                            │
├─────────────────────────────────────────────────────────────────────────┤
│  ┌─────────────┐  ┌──────────────┐  ┌─────────────┐  ┌───────────────┐  │
│  │   Axum      │  │   Request    │  │   Provider  │  │   Response    │  │
│  │   Router    │→ │   Transform  │→ │   Router    │→ │   Transform   │  │
│  └─────────────┘  └──────────────┘  └─────────────┘  └───────────────┘  │
│         │                                                     │         │
│         ▼                                                     ▼         │
│  ┌─────────────┐  ┌──────────────┐  ┌─────────────┐  ┌───────────────┐  │
│  │   Auth      │  │   Rate       │  │   Health    │  │   Telemetry   │  │
│  │   Layer     │  │   Limiter    │  │   Checker   │  │   (OTel)      │  │
│  └─────────────┘  └──────────────┘  └─────────────┘  └───────────────┘  │
└───────────────────────────────────┬─────────────────────────────────────┘
                                    │ gRPC / REST
                                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                          PROVIDER LAYER                                 │
├─────────────────────────────────────────────────────────────────────────┤
│  ┌──────────────────┐  ┌──────────────────┐  ┌──────────────────────┐   │
│  │   Vertex AI      │  │   Fallback:      │  │   Fallback:          │   │
│  │   (Primary)      │  │   DeepSeek       │  │   Local/Ollama       │   │
│  │                  │  │                  │  │                      │   │
│  │   Gemini 2.5     │  │   deepseek-chat  │  │   qwen2.5-coder      │   │
│  │   Gemini 3 Pro   │  │   deepseek-r1    │  │   codestral          │   │
│  └──────────────────┘  └──────────────────┘  └──────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## Corrections to Initial Design

| Issue | Initial Design | Corrected Design |
|-------|----------------|------------------|
| **Language** | Python (LiteLLM) | Rust native for performance and memory safety |
| **Model Versions** | Gemini 1.5 only | Gemini 2.5 Flash/Pro, Gemini 3 Pro Preview (Nov 2025) |
| **Quota System** | Fixed 1K RPM | Dynamic Shared Quota (DSQ) for Gemini 2.0+ models |
| **Cursor Connection** | HTTP localhost:4000 | **HTTPS required** - Cursor rejects localhost; needs public URL or tunnel |
| **Auth Tokens** | Static JSON key | Short-lived tokens (3600s max) with auto-refresh |
| **Google SDK** | Python SDK only | Official Google Cloud Rust SDK (released Sep 2025) |

---

## Technology Stack

### Core Framework

| Component | Technology | Rationale |
|-----------|------------|-----------|
| **Runtime** | Tokio | Industry standard async runtime for Rust |
| **Web Framework** | Axum 0.7+ | Tower-native, excellent middleware ecosystem |
| **HTTP Client** | Reqwest | Async, TLS support, connection pooling |
| **TLS** | Rustls | Pure Rust, modern cipher suites, no OpenSSL dependency |

### Google Cloud Integration

| Component | Technology | Rationale |
|-----------|------------|-----------|
| **Auth** | `gcloud-sdk` or Official `gcp-sdk` | Application Default Credentials (ADC), service account support |
| **Token Management** | Custom refresh layer | Handles 3600s token lifetime with proactive refresh |
| **API Format** | REST (generateContent) | Simpler than gRPC for proxy use case |

### LLM Abstraction Layer

| Component | Options | Notes |
|-----------|---------|-------|
| **Multi-Provider** | `rig-core` | Unified API across OpenAI, Anthropic, Gemini |
| **Protocol Adapter** | `llm-connector` | Lightweight protocol conversion |
| **Proxy Framework** | `chronicle-proxy` | OpenTelemetry, fallback, observability |

### Infrastructure

| Component | Technology | Rationale |
|-----------|------------|-----------|
| **Reverse Proxy** | `axum-reverse-proxy` | Seamless Tower middleware integration |
| **Config** | `config` + TOML/YAML | Hierarchical configuration |
| **Logging** | `tracing` + `tracing-subscriber` | Structured logging, OTel integration |
| **Metrics** | `metrics` + Prometheus exporter | Resource and cost tracking |

---

## Supported Models (November 2025)

### Vertex AI Gemini Models

| Model | Context | Pricing (per 1M tokens) | Notes |
|-------|---------|-------------------------|-------|
| `gemini-2.5-flash` | 1M | $0.15 input / $0.60 output | Best cost-efficiency |
| `gemini-2.5-pro` | 1M | $1.25 input / $5.00 output (≤200K) | Long context premium above 200K |
| `gemini-3-pro-preview` | 2M+ | Preview pricing (may be free) | Latest capabilities, Nov 2025 |

### Fallback Providers

| Provider | Model | Use Case |
|----------|-------|----------|
| DeepSeek | `deepseek-chat`, `deepseek-r1` | Cost-effective fallback |
| Ollama (Local) | `qwen2.5-coder`, `codestral` | Offline/air-gapped environments |

---

## Authentication Architecture

### Google Cloud Auth Flow

```text
┌────────────────┐     ┌──────────────────┐     ┌─────────────────┐
│  Service       │     │  Token           │     │  Vertex AI      │
│  Account JSON  │ ──▶ │  Refresh Manager │ ──▶ │  API            │
│  (or ADC)      │     │  (3600s cycle)   │     │                 │
└────────────────┘     └──────────────────┘     └─────────────────┘
        │                       │
        │  GOOGLE_APPLICATION_  │  Bearer Token
        │  CREDENTIALS          │  (auto-refresh)
        ▼                       ▼
┌─────────────────────────────────────────────────────────────────┐
│  Environment Variables:                                         │
│  - GOOGLE_APPLICATION_CREDENTIALS=/path/to/key.json             │
│  - GOOGLE_CLOUD_PROJECT=your-project-id                         │
│  - VERTEX_REGION=us-central1                                    │
└─────────────────────────────────────────────────────────────────┘
```

### Client Authentication (Cursor → Proxy)

```yaml
# Proxy accepts a static API key for client auth
auth:
  master_key: "sk-your-proxy-master-key"  # Used by all clients
  allowed_keys:
    - "sk-cursor-user-1"
    - "sk-cursor-user-2"
```

---

## Deployment Topology

### Option A: Local Development with Tunnel (Recommended for Personal Use)

```text
Cursor IDE
    │
    │ HTTPS
    ▼
┌────────────────┐     ┌────────────────┐     ┌────────────────┐
│   Cloudflare   │     │   vertex-      │     │   Vertex AI    │
│   Tunnel       │ ──▶ │   bridge       │ ──▶ │   (Gemini)     │
│   (cloudflared)│     │   localhost    │     │                │
└────────────────┘     └────────────────┘     └────────────────┘
```

**Why Cloudflare Tunnel over ngrok:**

- Free tier with no session limits
- Direct integration with Cloudflare DNS
- No random URL changes

### Option B: VPS Deployment (Production)

```text
Cursor IDE
    │
    │ HTTPS
    ▼
┌────────────────┐     ┌────────────────┐     ┌────────────────┐
│   Caddy/       │     │   vertex-      │     │   Vertex AI    │
│   Traefik      │ ──▶ │   bridge       │ ──▶ │   (Gemini)     │
│   (TLS term)   │     │   :4000        │     │                │
└────────────────┘     └────────────────┘     └────────────────┘
        │
        └── Let's Encrypt (auto-renewal)
```

---

## Configuration Schema

```toml
# vertex-bridge.toml

[server]
listen = "127.0.0.1:4000"
tls_cert = "/path/to/cert.pem"      # Optional: for direct TLS
tls_key = "/path/to/key.pem"

[auth]
master_key = "${PROXY_MASTER_KEY}"   # Environment variable reference
require_auth = true

[providers.vertex]
enabled = true
project_id = "${GOOGLE_CLOUD_PROJECT}"
region = "us-central1"
default_model = "gemini-2.5-flash"
credentials_path = "${GOOGLE_APPLICATION_CREDENTIALS}"

[providers.vertex.models]
"gemini-flash" = "gemini-2.5-flash-002"
"gemini-pro" = "gemini-2.5-pro-002"
"gemini-3" = "gemini-3-pro-preview"

[providers.deepseek]
enabled = true
api_key = "${DEEPSEEK_API_KEY}"
priority = 2                          # Fallback priority

[providers.ollama]
enabled = false
base_url = "http://localhost:11434"
priority = 3

[rate_limiting]
enabled = true
requests_per_minute = 100
tokens_per_minute = 1000000

[observability]
log_level = "info"
metrics_port = 9090
otlp_endpoint = "http://localhost:4317"  # Optional: OTel collector

[fallback]
enabled = true
strategy = "round_robin"              # or "priority", "random"
retry_count = 3
retry_delay_ms = 1000
```

---

## Cursor IDE Integration

### Settings Configuration

```json
{
  "openai.apiKey": "sk-cursor-user-1",
  "openai.baseURL": "https://your-domain.com/v1",
  "cursor.model": "gemini-flash"
}
```

### Important Notes

1. **HTTPS Required**: Cursor will not connect to HTTP endpoints or localhost
2. **Model Names**: Use the `model_name` from proxy config, not the full Vertex model ID
3. **Streaming**: SSE streaming is fully supported through the proxy
4. **Function Calling**: Translated between OpenAI and Vertex formats automatically

---

## Monitoring & Observability

### Key Metrics

| Metric | Description |
|--------|-------------|
| `llm_requests_total` | Total requests by provider, model, status |
| `llm_request_duration_seconds` | Latency histogram |
| `llm_tokens_total` | Input/output token counts |
| `llm_cost_usd` | Estimated cost tracking |
| `provider_health` | Provider availability gauge |

### Log Structure

```json
{
  "timestamp": "2025-11-29T12:00:00Z",
  "level": "info",
  "span": "request",
  "request_id": "req-abc123",
  "model": "gemini-2.5-flash",
  "input_tokens": 1500,
  "output_tokens": 500,
  "duration_ms": 342,
  "provider": "vertex",
  "status": "success"
}
```

---

## Security Considerations

### Network Security

- **TLS 1.3** enforced for all external connections
- **IP Allowlisting** optional for VPS deployments
- **mTLS** supported for enterprise environments

### Credential Security

- Service account keys stored outside repository
- Environment variable injection for secrets
- Short-lived tokens (3600s) with automatic refresh
- Audit logging for all API calls

### China Region Considerations

For deployments requiring access from China:

1. Use a proxy (Clash/V2Ray) with global mode
2. Vertex AI regions: `asia-northeast1` (Tokyo) has best latency
3. Consider Alibaba Cloud Qwen as regional fallback

---

## Performance Targets

| Metric | Target | Notes |
|--------|--------|-------|
| P50 Latency | <100ms | Proxy overhead only |
| P99 Latency | <500ms | Excluding LLM inference |
| Throughput | 1000 RPS | Per instance |
| Memory | <50MB | Baseline with connection pool |

---

## Future Enhancements

### Phase 2: Intelligent Routing

- Cost-based model selection
- Context-aware provider routing
- Prompt caching integration (Vertex supports this)

### Phase 3: Memory Layer

- Memento-style conversation persistence
- Prompt optimization based on usage patterns
- RAG integration for codebase-aware completions

---

## Quick Start Checklist

- [ ] Create Google Cloud project with billing enabled
- [ ] Enable Vertex AI API
- [ ] Create service account with `Vertex AI User` role
- [ ] Download service account JSON key
- [ ] Set environment variables
- [ ] Build and run proxy
- [ ] Set up tunnel (Cloudflare) or deploy to VPS
- [ ] Configure Cursor with proxy URL

---

## References

- [Google Cloud Rust SDK](https://github.com/googleapis/google-cloud-rust) (Official, Sep 2025)
- [Rig Framework](https://rig.rs/) - Rust LLM abstraction
- [Chronicle Proxy](https://github.com/dimfeld/chronicle) - Rust LLM proxy with observability
- [Vertex AI Gemini Pricing](https://cloud.google.com/vertex-ai/generative-ai/pricing)
- [Cursor Custom API Guide](https://www.cursor-ide.com/blog/cursor-custom-api-key-guide-2025)

---

*This document outlines the high-level architecture and technical decisions. Implementation details and code are provided in separate module documentation.*
