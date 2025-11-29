# Roadmap: Vertex AI LLM Proxy

> **Manifesto**: Build a robust, high-performance bridge to unlock Vertex AI for every developer.

## 🏆 Victory Conditions (Definition of Done)

1. **Core Functionality**: A user can point Cursor IDE to `http://localhost:4000/v1` and successfully chat with `gemini-2.5-flash`.
2. **Reliability**: The proxy handles token refresh automatically without user intervention.
3. **Performance**: P50 latency overhead < 50ms.
4. **Compatibility**: Supports both streaming (`stream=true`) and non-streaming responses.

## 🛠️ Tech Stack (Locked)

* **Language**: Rust (2024 Edition)
* **Web Framework**: Axum 0.7
* **HTTP Client**: Reqwest (with `rustls`)
* **Serialization**: Serde (JSON)
* **Observability**: Tracing + OpenTelemetry
* **Runtime**: Tokio

## 🪜 Phased Execution

### Phase 1: The Skeleton (Core)

* [ ] **Project Init**: `cargo new`, dependency setup (`axum`, `tokio`, `serde`, `reqwest`).
* [ ] **Configuration**: Implement `config-rs` to load `vertex-bridge.toml` and env vars.
* [ ] **Health Check**: `GET /health` endpoint.
* [ ] **Auth Middleware**: Validate `Authorization: Bearer <sk-...>` against config.
* [ ] **Logging**: Setup `tracing-subscriber` for structured logs.

### Phase 2: The Bridge (Feature)

* [ ] **Type Definitions**: Create Rust structs for OpenAI Request/Response and Vertex Request/Response.
* [ ] **Google Auth**: Implement `TokenManager` to fetch/refresh Google OAuth2 tokens (Service Account/ADC).
* [ ] **Translation Layer**:
  * [ ] `OpenAI -> Vertex`: Convert messages, temperature, max_tokens.
  * [ ] `Vertex -> OpenAI`: Convert candidates, usage metadata.
* [ ] **Proxy Handler**: Implement `POST /v1/chat/completions`.
  * [ ] Unary (Non-streaming) support.
  * [ ] Streaming (SSE) support.

### Phase 3: Resilience & Polish

* [ ] **Error Handling**: Map Vertex errors (400, 401, 429) to OpenAI-compatible error responses.
* [ ] **Rate Limiting**: Implement in-memory token bucket (governor).
* [ ] **Fallback Logic**: Basic structure for switching providers on failure (DeepSeek/Ollama).
* [ ] **Integration Tests**: Test against real Vertex API (mocked and live).

### Phase 4: Ship

* [ ] **Docker**: Optimized `Dockerfile` (distroless/cc).
* [ ] **Release**: Binary builds for macOS/Linux.
* [ ] **Documentation**: Usage guide for Cursor/VSCode.

## 📂 Directory Structure

```text
.
├── src/
│   ├── bin/            # Entry points
│   ├── config/         # Configuration loading
│   ├── handlers/       # API Route Handlers
│   ├── models/         # Request/Response Structs (OpenAI/Vertex)
│   ├── services/       # Business Logic (Vertex Client, Auth)
│   ├── middleware/     # Auth, Logging, Rate Limiting
│   └── utils/          # Helpers
├── tests/              # Integration Tests
├── docs/               # Documentation
└── infra/              # Docker, K8s
```
