# Task Breakdown: Vertex AI LLM Proxy

## Slice 1: The Walking Skeleton (Server Foundation)

- [x] [CONF] Initialize `Cargo.toml` with `axum`, `tokio`, `serde`, `reqwest`, `tracing`, `config`.
- [ ] [CONF] Create `vertex-bridge.toml` with default settings (port, log level). *Optional: env vars work*
- [x] [CODE] Implement `src/config/mod.rs` to load configuration from file and env vars.
- [x] [CODE] Setup `src/lib.rs` and `src/main.rs` with `tokio` runtime and `tracing-subscriber`.
- [x] [API] Implement `GET /health` handler in `src/handlers/health.rs`.
- [ ] [TEST] Verify server starts on port 4000 and responds to `/health`.

## Slice 2: Data Models & Google Auth (The Core)

- [x] [CODE] Define `ChatCompletionRequest` struct (OpenAI format) in `src/models/openai.rs`.
- [x] [CODE] Define `GenerateContentRequest` struct (Vertex format) in `src/models/vertex.rs`.
- [x] [CODE] Implement `TokenManager` in `src/services/auth.rs` to handle Google Service Account JSON.
- [x] [CODE] Implement `get_token()` method with caching and auto-refresh logic.
- [ ] [TEST] Unit test `TokenManager` (mocked) and integration test (real credential).

## Slice 3: The Bridge (Proxy Logic)

- [x] [CODE] Implement `transform_request` (OpenAI -> Vertex) in `src/services/transformer.rs`.
- [x] [CODE] Implement `transform_response` (Vertex -> OpenAI) for unary responses.
- [x] [API] Create `POST /v1/chat/completions` handler in `src/handlers/chat.rs`.
- [x] [API] Wire up provider abstraction with `VertexProvider` and `ProviderRegistry`.
- [ ] [TEST] End-to-end test with `curl` using a real Vertex model. *Scripts exist, need formal tests*

## Slice 4: Streaming & Polish

- [x] [CODE] Implement SSE (Server-Sent Events) support for `stream=true`.
- [x] [CODE] Implement `transform_stream_chunk` (Vertex Stream -> OpenAI Chunk).
- [x] [CODE] Add `Authorization` middleware to protect the proxy with `master_key`.
- [x] [INFRA] Create `Dockerfile` for production build.

---

## Next Steps: Testing & Hardening

### Priority 1: Unit Tests

- [x] [TEST] `src/services/auth.rs` - TokenManager with API key and project ID (3 tests)
- [x] [TEST] `src/services/transformer.rs` - Request/response transformation (4 tests)
- [x] [TEST] `src/services/providers/mod.rs` - Provider routing and registry (5 tests)
- [x] [TEST] `src/middleware/auth.rs` - Auth middleware config validation (3 tests)
- [x] [TEST] `src/services/providers/anthropic.rs` - All tests passing (24/24)
- [x] [TEST] `src/services/providers/vertex.rs` - All tests passing (8/8)

### Priority 2: Integration Tests

- [x] [TEST] `tests/integration/health_test.rs` - Health endpoint (@critical)
- [x] [TEST] `tests/integration/chat_test.rs` - Chat completions (non-streaming & streaming) (@critical, requires credentials)
- [x] [TEST] `tests/integration/auth_test.rs` - Auth middleware E2E (6 tests, @critical)
- [x] [TEST] `tests/integration/error_test.rs` - Error handling scenarios (@critical)
- [x] [TEST] `tests/integration/smoke_test.rs` - Fast sanity checks (< 2 min, @smoke)
- [x] [TEST] `tests/integration/rate_limit_test.rs` - Rate limiting verification (3 tests)
- [x] [TEST] `tests/integration/metrics_test.rs` - Metrics endpoint (2 tests)
- [x] [TEST] Test infrastructure: `TestServer` utility, CI integration, test scripts
- [x] [TEST] **Total: 26 tests passing, 2 ignored (credentials required)**

### Priority 3: Error Handling

- [x] [CODE] Fixed unreachable pattern warnings in error mapping
- [x] [CODE] Added error context to provider HTTP requests
- [x] [CODE] Added timeout configuration (30s non-streaming, 60s streaming)
- [x] [CODE] Add error context to all `?` operator usages in handlers (handlers use match statements, providers already have context)
- [x] [TEST] Test error scenarios (401, 403, 429, 500, malformed requests) - `error_test.rs`

### Priority 4: Documentation

- [x] [DOCS] Update README with testing instructions
- [x] [DOCS] Add architecture diagram (`docs/dev/architecture/system-overview.md`)
- [x] [DOCS] Document provider abstraction pattern (`docs/dev/architecture/provider-pattern.md`)
- [x] [DOCS] Create testing guide (`docs/dev/testing/guide.md`)
