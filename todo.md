# Task Breakdown: Vertex AI LLM Proxy

## Slice 1: The Walking Skeleton (Server Foundation)

- [x] [CONF] Initialize `Cargo.toml` with `axum`, `tokio`, `serde`, `reqwest`, `tracing`, `config`.
- [x] [CONF] Create `vertex-bridge.toml` with default settings (port, log level).
- [x] [CODE] Implement `src/config/mod.rs` to load configuration from file and env vars.
- [x] [CODE] Setup `src/lib.rs` and `src/main.rs` with `tokio` runtime and `tracing-subscriber`.
- [x] [API] Implement `GET /health` handler in `src/handlers/health.rs`.
- [x] [TEST] Verify server starts on port 4000 and responds to `/health`.

## Slice 2: Data Models & Google Auth (The Core)

- [x] [CODE] Define `ChatCompletionRequest` struct (OpenAI format) in `src/models/openai.rs`.
- [x] [CODE] Define `GenerateContentRequest` struct (Vertex format) in `src/models/vertex.rs`.
- [x] [CODE] Implement `TokenManager` in `src/services/auth.rs` to handle Google Service Account JSON.
- [x] [CODE] Implement `get_token()` method with caching and auto-refresh logic.
- [x] [TEST] Unit test `TokenManager` (mocked) and integration test (real credential).

## Slice 3: The Bridge (Proxy Logic)

- [x] [CODE] Implement `transform_request` (OpenAI -> Vertex) in `src/services/transformer.rs`.
- [x] [CODE] Implement `transform_response` (Vertex -> OpenAI) for unary responses.
- [x] [API] Create `POST /v1/chat/completions` handler in `src/handlers/chat.rs`.
- [x] [API] Wire up `VertexClient` to send requests using the auth token.
- [x] [TEST] End-to-end test with `curl` using a real Vertex model.

## Slice 4: Streaming & Polish

- [x] [CODE] Implement SSE (Server-Sent Events) support for `stream=true`.
- [x] [CODE] Implement `transform_stream_chunk` (Vertex Stream -> OpenAI Chunk).
- [x] [CODE] Add `Authorization` middleware to protect the proxy with `master_key`.
- [x] [INFRA] Create `Dockerfile` for production build.
