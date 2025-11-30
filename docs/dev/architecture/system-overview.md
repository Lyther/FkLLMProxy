# Architecture Overview

## System Components

```text
┌─────────────────────────────────────────────────────────────────┐
│                         Client (OpenAI SDK)                     │
└────────────────────────────┬────────────────────────────────────┘
                             │
                             │ HTTP/SSE
                             │
┌────────────────────────────▼────────────────────────────────────┐
│                    Vertex Bridge (Rust)                         │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │              API Handlers (Axum)                         │   │
│  │  - /v1/chat/completions (OpenAI-compatible)              │   │
│  │  - /health                                               │   │
│  │  - /metrics                                              │   │
│  │  - /metrics/prometheus                                   │   │
│  └────────────┬──────────────────────────────┬──────────────┘   │
│               │                              │                  │
│  ┌────────────▼──────────┐      ┌────────────▼──────────┐       │
│  │  Provider Registry    │      │   Middleware          │       │
│  │  - Route by model     │      │   - Auth              │       │
│  │  - Provider selection │      │   - Rate limiting     │       │
│  └────────────┬──────────┘      │   - Metrics           │       │
│               │                 └───────────────────────┘       │
│  ┌────────────▼─────────────────────────────────────────┐       │
│  │              Provider Abstraction                    │       │
│  │  ┌──────────────┐  ┌──────────────┐  ┌─────────────┐ │       │
│  │  │ Anthropic    │  │   Vertex     │  │   OpenAI    │ │       │
│  │  │ Provider     │  │   Provider   │  │   Provider  │ │       │
│  │  └──────┬───────┘  └──────┬───────┘  └───────┬─────┘ │       │
│  └─────────┼─────────────────┼──────────────────┼───────┘       │
│            │                 │                  │               │
└────────────┼─────────────────┼──────────────────┼───────────────┘
             │                 │                  │
    ┌────────▼────────┐ ┌──────▼───────┐  ┌──────▼──────┐
    │ Anthropic       │ │ Vertex AI    │  │ OpenAI      │
    │ Bridge (Node.js)│ │ API          │  │ API         │
    └─────────────────┘ └──────────────┘  └─────────────┘
```

## Data Flow

### Request Flow (Non-Streaming)

1. **Client Request** → OpenAI-compatible request to `/v1/chat/completions`
2. **Handler** → Routes to appropriate provider based on model name
3. **Provider** → Transforms request format (OpenAI → Provider-specific)
4. **Provider** → Executes HTTP request to external API
5. **Provider** → Transforms response format (Provider-specific → OpenAI)
6. **Handler** → Returns OpenAI-compatible response

### Request Flow (Streaming)

1. **Client Request** → OpenAI-compatible streaming request
2. **Handler** → Routes to provider
3. **Provider** → Executes streaming HTTP request
4. **Provider** → Streams SSE chunks, transforming on-the-fly
5. **Handler** → Forwards SSE chunks to client

## Component Details

### API Handlers (`src/handlers/`)

**chat.rs**: Main handler for non-OpenAI models

- Routes to Anthropic or Vertex providers
- Handles streaming and non-streaming requests
- Transforms provider responses to OpenAI format

**openai_chat.rs**: Handler for OpenAI models

- Routes to OpenAI backend via harvester
- Handles token management and Arkose challenges
- Transforms OpenAI responses

**health.rs**: Health check endpoint

- Returns service status
- Checks dependencies

**metrics.rs**: Metrics endpoint

- Exposes Prometheus-format metrics

### Provider Abstraction (`src/services/providers/`)

**Trait**: `Provider`

```rust
pub trait Provider: Send + Sync {
    fn provider_type(&self) -> Provider;
    fn supports_model(&self, model: &str) -> bool;
    async fn execute(&self, request: ChatCompletionRequest, state: &AppState) -> ProviderResult<ChatCompletionResponse>;
    async fn execute_stream(&self, request: ChatCompletionRequest, state: &AppState) -> ProviderResult<StreamingResponse>;
}
```

**Implementations**:

- `AnthropicBridgeProvider`: Communicates with Node.js bridge
- `VertexProvider`: Direct API integration with Vertex AI
- `OpenAIProvider`: Direct API integration (future)

### Middleware (`src/middleware/`)

**Auth**: Validates API keys

- Optional authentication
- Master key support

**Rate Limiting**: Token bucket algorithm

- Global rate limiting
- Configurable capacity and refill rate

**Metrics**: Request tracking

- Success/failure counts
- Duration tracking
- Provider-specific metrics

### Circuit Breaker (`src/openai/circuit_breaker.rs`)

**States**:

- `Closed`: Normal operation
- `Open`: Failing, rejecting requests
- `HalfOpen`: Testing recovery

**Transitions**:

- Closed → Open: Failure threshold reached
- Open → HalfOpen: Timeout expired
- HalfOpen → Closed: Success threshold reached
- HalfOpen → Open: Failure during test

### Token Management (`src/services/auth.rs`)

**Vertex Provider**:

- API Key: Direct authentication
- OAuth: Service account credentials
- Automatic token refresh

**OpenAI Provider**:

- Harvester service integration
- Arkose token solving for GPT-4

## Configuration

Configuration is loaded from environment variables with `APP_` prefix:

```bash
APP_SERVER__HOST=0.0.0.0
APP_SERVER__PORT=4000
APP_VERTEX__PROJECT_ID=my-project
APP_VERTEX__REGION=us-central1
APP_ANTHROPIC__BRIDGE_URL=http://localhost:4001
```

See `.env.example` for full configuration options.

## Error Handling

**Error Types**:

- `ProviderError::Auth`: Authentication failures
- `ProviderError::Network`: Network/connection errors
- `ProviderError::Unavailable`: Service unavailable (503)
- `ProviderError::InvalidRequest`: Bad request (400)
- `ProviderError::RateLimited`: Rate limit exceeded (429)
- `ProviderError::Internal`: Internal server errors (500)

**Error Flow**:

1. Provider errors are converted to `ProviderError`
2. Handlers map `ProviderError` to HTTP status codes
3. Errors are logged with context
4. Client receives OpenAI-compatible error format

## Testing

**Unit Tests**: 48 tests covering:

- Provider implementations
- Error handling
- Edge cases
- Circuit breaker behavior

**Integration Tests**: 26 tests covering:

- End-to-end request flows
- Multi-provider routing
- Error scenarios

**Test Infrastructure**:

- `wiremock` for HTTP mocking
- Reusable test helpers
- BDD-style test patterns

## Deployment

**Docker Compose**: Multi-service setup

- `vertex-bridge`: Main Rust service
- `anthropic-bridge`: Node.js bridge service
- `harvester`: Token harvesting service

**Production Considerations**:

- Health checks configured
- Graceful shutdown (Implemented)
- Metrics exposed
- Structured logging (Implemented)

## Recent Enhancements

1. **TLS Fingerprinting**: Configuration structure in place (see [ADR 005](../adr/005-tls-fingerprinting.md))
2. **Production Deployment**: Kubernetes manifests and production Docker Compose
3. **Monitoring**: Comprehensive Prometheus metrics and monitoring guide
4. **Response Compression**: Gzip/Brotli compression middleware enabled
5. **Security**: Automated security audit scripts and comprehensive security review

## Future Enhancements

1. **Full TLS Fingerprinting**: Complete reqwest-impersonate integration
2. **Distributed Tracing**: OpenTelemetry integration (dependencies added)
3. **Request Caching**: Response caching layer with Redis or in-memory
4. **Load Balancing**: Multiple provider instances
5. **API Documentation**: Auto-generated OpenAPI specs with utoipa
