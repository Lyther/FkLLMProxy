# Roadmap: Iron-Clad Proxy (ICP) - OpenAI Web Interface Bridge

> **Manifesto**: Build a split-process proxy that bridges OpenAI-compatible clients to ChatGPT Web Interface via TLS fingerprinting and browser automation.

## 🏆 Victory Conditions (Definition of Done)

1. **Core Functionality**: A user can point Cursor IDE to `http://localhost:4000/v1` and successfully chat with `gpt-4` or `gpt-3.5-turbo` through the ChatGPT Web Interface.
2. **TLS Fingerprinting**: (PLANNED - NOT YET IMPLEMENTED) The Enforcer would bypass Cloudflare WAF using `reqwest-impersonate` with Chrome v120+ ClientHello signatures. Currently using standard `reqwest` which may be blocked by WAF.
3. **Session Management**: The Harvester maintains a valid browser session and automatically refreshes `access_token` and `arkose_token` without user intervention.
4. **Streaming**: Supports SSE streaming with < 6s first token latency (worst case with Arkose solve).
5. **Reliability**: Handles WAF blocks, token expiration, and browser crashes with automatic recovery.

## 🛠️ Tech Stack (Locked)

### The Enforcer (Rust)

* **Language**: Rust (2024 Edition, `edition = "2021"`)
* **Web Framework**: Axum 0.7
* **HTTP Client**: `reqwest` 0.11 (Standard TLS - `reqwest-impersonate` planned but not yet implemented)
* **Serialization**: Serde (JSON)
* **Observability**: Tracing + OpenTelemetry
* **Runtime**: Tokio
* **Error Handling**: `anyhow` + `thiserror` (zero `unwrap()`)

### The Harvester (TypeScript)

* **Language**: TypeScript (Node.js 22+)
* **Browser Automation**: Playwright 1.40+ (Chromium)
* **HTTP Server**: Fastify 4.24+
* **Logging**: Pino (structured JSON logs)
* **Runtime**: Node.js ESM modules

## 🪜 Phased Execution

### Phase 1: The Enforcer Skeleton (Core)

* [ ] **Project Structure**: Create `src/openai/` module structure parallel to existing `vertex` code.
* [ ] **Dependencies**: Add `reqwest-impersonate` to `Cargo.toml` (fork with BoringSSL).
* [ ] **Configuration**: Extend `AppConfig` with OpenAI-specific settings:
  * `openai.harvester_url` (default: `http://localhost:3001`)
  * `openai.impersonation_profile` (Chrome120, Edge119, etc.)
  * `openai.token_cache_ttl` (arkose: 2min, access: 1hr)
* [ ] **Health Check**: Extend `/health` to include Harvester connectivity check.
* [ ] **Logging**: Setup structured logging for OpenAI requests (separate from Vertex).

### Phase 2: The Harvester (Session Manager)

* [ ] **Project Init**: Create `harvester/` directory with `package.json`, `tsconfig.json`.
* [ ] **Dependencies**: Install `playwright`, `fastify`, `pino`, `tsx`.
* [ ] **Browser Launch**: Implement Playwright Chromium instance with persistent context.
* [ ] **Session Initialization**:
  * [ ] Manual login flow (user provides cookies or performs OAuth).
  * [ ] Cookie persistence (save to `harvester/cookies.json`).
  * [ ] Session validation (`GET /api/auth/session`).
* [ ] **Token Extraction**:
  * [ ] Intercept `fetch` to `/api/auth/session` via CDP.
  * [ ] Extract `accessToken` from response.
  * [ ] Cache token with TTL.
* [ ] **HTTP API**: Implement Fastify server with endpoints:
  * [ ] `GET /tokens` → Returns `{ access_token, arkose_token?, expires_at }`
  * [ ] `POST /refresh` → Forces token refresh
  * [ ] `GET /health` → Browser instance health

### Phase 3: The Bridge (Request Transformation) ✅ COMPLETE

* [x] **Type Definitions**: Created Rust structs in `src/openai/models.rs`:
  * [x] `BackendConversationRequest` (internal format)
  * [x] `BackendSSEEvent` (SSE parsing)
  * [x] `TokenResponse` (Harvester API response)
* [x] **Harvester Client**: Implemented `HarvesterClient` in `src/openai/harvester.rs`:
  * [x] `get_tokens()` → HTTP client to `localhost:3001/tokens`
  * [x] Token cache with TTL (in-memory `HashMap`)
  * [x] Error handling (503 if Harvester unavailable)
* [x] **Request Transformer**: Implemented `transform_to_backend()` in `src/openai/transformer.rs`:
  * [x] OpenAI `messages[]` → Backend `node_id` structure
  * [x] Map `content: string` → `content: { parts: string[] }`
  * [x] Preserve `temperature`, `max_tokens`, `stream`
* [x] **Response Transformer**: Implemented `transform_sse_to_openai_chunk()`:
  * [x] Parse raw SSE events (`event: message`, `event: done`)
  * [x] Filter internal metadata (moderation flags, internal events)
  * [x] Transform to OpenAI `ChatCompletionChunk` format

### Phase 4: TLS Impersonation & Upstream Request (PARTIAL)

* [ ] **TLS Configuration**: Implement `reqwest-impersonate` client builder:
  * [ ] Select impersonation profile from config (Chrome120 default)
  * [ ] Configure cipher suites, extensions, JA3/JA4 matching
  * [ ] Test against Cloudflare WAF (verify 200 response, not 403)
  * **Status**: NOT IMPLEMENTED - Currently using standard `reqwest` (see TODO in `src/openai/backend.rs`)
* [x] **Header Ordering**: Implement strict header ordering:
  * [x] `User-Agent` (Chrome v120+)
  * [x] `Accept-Language` (en-US,en;q=0.9)
  * [x] `Referer` (<https://chatgpt.com/>)
  * [x] `Authorization: Bearer {access_token}`
  * [x] `Openai-Sentinel-Arkose-Token: {arkose_token}` (if GPT-4)
* [x] **Upstream Client**: Implemented `OpenAIBackendClient` in `src/openai/backend.rs`:
  * [x] `POST https://chatgpt.com/backend-api/conversation`
  * [x] SSE stream parsing (via `SSEParser`)
  * [x] Error handling (401 → refresh token, 403 → WAF block detection, 429 → rate limit)

### Phase 5: Arkose Token Generation (The Hard Part)

* [ ] **Arkose Trigger**: Implement JavaScript injection in Harvester:
  * [ ] Navigate to ChatGPT Web Interface
  * [ ] Inject script to call `window.arkose` callback
  * [ ] Extract `arkose_token` from callback response
  * [ ] Cache with 2-minute TTL
* [ ] **FunCaptcha Solver**: If manual solve required:
  * [ ] Detect FunCaptcha challenge in DOM
  * [ ] Pause automation and wait for user interaction (or integrate solver service)
  * [ ] Resume after token extraction
* [ ] **Token Refresh Logic**: Implement automatic refresh:
  * [ ] Check `arkose_token` expiration before GPT-4 requests
  * [ ] Trigger Harvester refresh if expired
  * [ ] Retry request with fresh token

### Phase 6: SSE Streaming & Handler ✅ COMPLETE

* [x] **Proxy Handler**: Implemented `POST /v1/chat/completions` in `src/handlers/openai_chat.rs`:
  * [x] Extract OpenAI request format
  * [x] Get tokens from Harvester (with cache)
  * [x] Transform request to backend format
  * [x] Execute upstream request (TLS impersonation planned but not yet implemented)
  * [x] Stream SSE chunks back to client
* [x] **Stream Transformation**: Real-time conversion:
  * [x] Parse backend SSE events
  * [x] Transform each chunk to OpenAI format
  * [x] Handle `[DONE]` event
  * [x] Error propagation (map backend errors to OpenAI format)
* [x] **Provider Router**: Extended existing router to detect OpenAI models:
  * [x] Route `gpt-4*` and `gpt-3.5-turbo*` to OpenAI handler
  * [x] Keep existing Vertex routing for `gemini-*`

### Phase 7: Resilience & Polish

* [ ] **Error Handling**: Map backend errors to OpenAI format:
  * [ ] 401 → `{ error: { type: "invalid_request_error", message: "Invalid authentication" } }`
  * [ ] 403 → Log WAF block, switch impersonation profile, retry
  * [ ] 429 → `{ error: { type: "rate_limit_error" } }`
* [ ] **Rate Limiting**: Implement in-memory token bucket per IP/Key.
* [ ] **Session Keep-Alive**: Implement Harvester background task:
  * [ ] Navigate or interact every 5-10 minutes
  * [ ] Refresh session if invalid
* [ ] **Circuit Breaker**: Track WAF block rate, disable OpenAI provider if > 10% failure.
* [ ] **Integration Tests**: Test against real ChatGPT Web Interface (with test account).

### Phase 8: Ship

* [ ] **Docker**: Create `Dockerfile.harvester` for TypeScript service.
* [ ] **Docker Compose**: Extend `docker-compose.yml` to run both Enforcer and Harvester.
* [ ] **Documentation**: Usage guide for Cursor/VSCode configuration.
* [ ] **Release**: Binary builds for Enforcer (Rust), npm package for Harvester.
* [ ] **Monitoring**: Add metrics for token cache hit rate, Arkose solve time, WAF block rate.

## 📂 Directory Structure

```text
.
├── src/
│   ├── openai/              # NEW: OpenAI-specific modules
│   │   ├── mod.rs
│   │   ├── models.rs        # BackendConversationRequest, BackendSSEEvent
│   │   ├── harvester.rs     # HarvesterClient (HTTP client to Node service)
│   │   ├── backend.rs       # OpenAIBackendClient (upstream requests)
│   │   └── transformer.rs   # Request/Response transformation
│   ├── handlers/
│   │   └── openai_chat.rs   # NEW: POST /v1/chat/completions (OpenAI path)
│   ├── config/
│   │   └── mod.rs           # EXTEND: Add OpenAI config section
│   └── ...                   # Existing Vertex code
├── harvester/                # NEW: TypeScript service
│   ├── src/
│   │   ├── index.ts         # Fastify server entry point
│   │   ├── browser.ts       # Playwright browser management
│   │   ├── session.ts       # Session initialization & keep-alive
│   │   ├── tokens.ts        # Token extraction & caching
│   │   └── arkose.ts        # Arkose token generation
│   ├── package.json
│   ├── tsconfig.json
│   └── cookies.json         # Persistent session storage
├── docs/
│   └── openai/
│       ├── openai-overview.md
│       ├── system-design-openai.md
│       └── roadmap.md       # This file
└── ...                       # Existing project files
```

## 🚨 Critical Path (10% Manual Work)

1. **TLS Handshake Config**: Tuning `reqwest-impersonate` to match Playwright browser exactly.
2. **Stream Transformer**: Rust state machine for backend SSE → OpenAI chunks (prone to breaking changes).
3. **Arkose Trigger**: JavaScript injection to force `window.arkose` callback without user click.

## ⚠️ Constraints

* **Time**: Split-process architecture requires coordination between Rust and TypeScript.
* **Legacy**: Must integrate with existing Vertex proxy code (shared router, config, logging).
* **WAF**: Cloudflare fingerprint blocking is a moving target (runtime configurable profiles required).

## 🎯 Success Metrics

* **Latency**: P50 first token < 1s (cached tokens), P95 < 6s (with Arkose solve).
* **Reliability**: 99% success rate (excluding WAF blocks, which require profile switching).
* **Uptime**: Harvester browser session stays alive > 24 hours without manual intervention.
