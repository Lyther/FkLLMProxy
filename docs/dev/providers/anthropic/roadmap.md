# Roadmap: Multi-Provider LLM Proxy

> **Manifesto**: Build a unified, high-performance bridge connecting OpenAI-compatible clients to multiple LLM providers (Vertex AI, Anthropic CLI, DeepSeek, Ollama).

## 🏆 Victory Conditions (Definition of Done)

**Primary**: A user can point Cursor IDE to `http://localhost:4000/v1` and successfully chat with any supported model (`gemini-*`, `claude-*`, `deepseek-*`, `ollama-*`).

**Success Metrics**:

1. **Multi-Provider Routing**: Model name determines provider automatically.
2. **Anthropic CLI Integration**: `claude-*` models work via stdio bridge (Node.js service).
3. **Reliability**: Token refresh, error handling, circuit breakers.
4. **Performance**: P50 latency overhead < 50ms (Vertex), < 100ms (Anthropic bridge).
5. **Compatibility**: Streaming and non-streaming responses for all providers.

## 🛠️ Tech Stack (Locked)

**Core Proxy (Rust)**:

- **Language**: Rust (2024 Edition)
- **Web Framework**: Axum 0.7
- **HTTP Client**: Reqwest (rustls)
- **Serialization**: Serde (JSON)
- **Observability**: Tracing + OpenTelemetry
- **Runtime**: Tokio

**Anthropic Bridge (Node.js/TypeScript)**:

- **Language**: TypeScript
- **Runtime**: Node.js
- **Framework**: Express
- **ANSI Stripping**: strip-ansi
- **Process Management**: child_process (spawn)

**No "Resume Driven Development"**: Boring, proven technology only.

## 🪜 Phased Execution

### Phase 1: The Skeleton (Core) ✅ COMPLETE

- [x] **Project Init**: `cargo new`, dependency setup (`axum`, `tokio`, `serde`, `reqwest`).
- [x] **Configuration**: Implement `config-rs` to load config from TOML and env vars.
- [x] **Health Check**: `GET /health` endpoint.
- [x] **Auth Middleware**: Validate `Authorization: Bearer <sk-...>` against config.
- [x] **Logging**: Setup `tracing-subscriber` for structured logs.

### Phase 2: The Bridge (Feature) ✅ VERTEX AI COMPLETE

**Vertex AI (Done)**:

- [x] **Type Definitions**: Rust structs for OpenAI Request/Response and Vertex Request/Response.
- [x] **Google Auth**: `TokenManager` to fetch/refresh Google OAuth2 tokens (Service Account/ADC).
- [x] **Translation Layer**:
  - [x] `OpenAI -> Vertex`: Convert messages, temperature, max_tokens.
  - [x] `Vertex -> OpenAI`: Convert candidates, usage metadata.
- [x] **Proxy Handler**: `POST /v1/chat/completions`.
  - [x] Unary (Non-streaming) support.
  - [x] Streaming (SSE) support.

**Anthropic CLI Bridge (✅ COMPLETE)**:

- [x] **Provider Router**: Model name → provider mapping (`claude-*` → AnthropicCLI).
- [x] **Anthropic Bridge Service**: Node.js/TypeScript stdio-to-HTTP bridge.
  - [x] Express server on internal port (4001).
  - [x] `POST /anthropic/chat` endpoint.
  - [x] Message concatenation (`messages[]` → prompt string).
  - [x] `spawn('claude', ['-p', prompt])` with ANSI stripping.
  - [x] SSE chunk wrapping (OpenAI format).
- [x] **Rust Integration**: HTTP client to call Anthropic bridge.
  - [x] `reqwest` client for internal bridge communication.
  - [x] Stream forwarding (bridge SSE → proxy SSE).
- [x] **Error Handling**: CLI errors (stderr) → OpenAI error format.

**Provider Abstraction (✅ COMPLETE)**:

- [x] **Provider Trait**: Define `Provider` trait with `execute()` method.
- [x] **Provider Registry**: Map model names to provider instances.
- [x] **Router Middleware**: Route requests based on model name.

### Phase 3: Resilience & Polish

- [ ] **Error Handling**: Map provider errors (400, 401, 429) to OpenAI-compatible responses.
- [ ] **Rate Limiting**: In-memory token bucket per IP/API key (governor crate).
- [ ] **Circuit Breaker**: Health state tracking per provider.
- [ ] **Fallback Logic**: Automatic provider switching on failure (Vertex → DeepSeek → Ollama).
- [ ] **Timeout Handling**: Request timeouts per provider (different for CLI vs HTTP).
- [ ] **Integration Tests**: Test against real providers (mocked and live).

### Phase 4: Ship

- [ ] **Docker**: Multi-stage `Dockerfile` (Rust proxy + Node.js bridge).
  - [ ] Rust binary (distroless/cc base).
  - [ ] Node.js bridge (alpine base).
  - [ ] `docker-compose.yml` for local development.
- [ ] **Release**: Binary builds for macOS/Linux (Rust proxy).
- [ ] **Documentation**:
  - [x] System design (`docs/anthropic/system-design.md`).
  - [x] API contract (`docs/api/api-contract.ts`).
  - [ ] User guide for Cursor/VSCode setup.
  - [ ] Provider configuration guide.
- [ ] **CI/CD**: GitHub Actions for tests, builds, releases.

## 📂 Directory Structure

```text
.
├── src/                    # Rust proxy core
│   ├── config/             # Configuration loading
│   ├── handlers/          # API Route Handlers
│   │   ├── chat.rs        # Main chat completions handler
│   │   └── health.rs      # Health check
│   ├── models/            # Request/Response Structs
│   │   ├── openai.rs      # OpenAI format
│   │   └── vertex.rs      # Vertex format
│   ├── services/          # Business Logic
│   │   ├── auth.rs        # Token management
│   │   ├── transformer.rs # Format conversion
│   │   └── providers/     # Provider implementations
│   │       ├── mod.rs     # Provider registry and routing logic
│   │       ├── vertex.rs
│   │       └── anthropic.rs
│   ├── middleware/         # Auth, Logging, Rate Limiting
│   └── utils/             # Helpers
├── bridge/                # Anthropic CLI bridge (NEW)
│   ├── src/
│   │   └── index.ts       # Express server
│   ├── package.json
│   └── tsconfig.json
├── tests/                  # Integration Tests
├── docs/                   # Documentation
│   ├── anthropic/
│   ├── openai/
│   └── api/
└── infra/                  # Docker, K8s
    ├── Dockerfile
    └── docker-compose.yml
```

## 🎯 Immediate Next Steps

1. **Create Provider Abstraction**:
   - Define `Provider` trait in `src/services/providers/mod.rs`.
   - Implement `VertexProvider` (refactor existing code).
   - Create `AnthropicBridgeProvider` (HTTP client to bridge).

2. **Build Anthropic Bridge**:
   - Initialize Node.js project in `bridge/`.
   - Implement Express server with `/anthropic/chat` endpoint.
   - Add stdio capture and ANSI stripping.

3. **Integrate Router**:
   - Update `chat.rs` handler to use provider router.
   - Route by model name prefix.

4. **Test End-to-End**:
   - Verify `claude-3-5-sonnet` works via bridge.
   - Verify `gemini-2.0-flash` still works via Vertex.

---

**Status**: Phase 1 & 2 (Vertex AI & Anthropic CLI Bridge) complete. Phase 3 (Resilience & Polish) in progress.
