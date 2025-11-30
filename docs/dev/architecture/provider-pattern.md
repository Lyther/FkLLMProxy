# Provider Abstraction Pattern

## Overview

The provider abstraction pattern allows the system to support multiple LLM providers (Anthropic, Vertex AI, OpenAI) through a unified interface while maintaining provider-specific implementations.

## Architecture

### Trait Definition

```rust
#[async_trait]
pub trait Provider: Send + Sync {
    fn provider_type(&self) -> Provider;
    fn supports_model(&self, model: &str) -> bool;

    async fn execute(
        &self,
        request: ChatCompletionRequest,
        state: &AppState,
    ) -> ProviderResult<ChatCompletionResponse>;

    async fn execute_stream(
        &self,
        request: ChatCompletionRequest,
        state: &AppState,
    ) -> ProviderResult<StreamingResponse>;
}
```

### Key Design Principles

1. **Unified Interface**: All providers implement the same trait
2. **Model-Based Routing**: Providers declare which models they support
3. **Request Transformation**: Providers transform OpenAI format to their native format
4. **Response Transformation**: Providers transform their format back to OpenAI format
5. **Error Consistency**: All providers return `ProviderError` types

## Implementation Pattern

### 1. Provider Registration

```rust
pub struct ProviderRegistry {
    providers: Vec<Box<dyn Provider>>,
}

impl ProviderRegistry {
    pub fn route_by_model(&self, model: &str) -> Option<&dyn Provider> {
        self.providers.iter()
            .find(|p| p.supports_model(model))
            .map(|p| p.as_ref())
    }
}
```

### 2. Provider Implementation

**Example: Vertex Provider**

```rust
pub struct VertexProvider;

impl Provider for VertexProvider {
    fn provider_type(&self) -> Provider {
        Provider::Vertex
    }

    fn supports_model(&self, model: &str) -> bool {
        model.starts_with("gemini-")
    }

    async fn execute(
        &self,
        request: ChatCompletionRequest,
        state: &AppState,
    ) -> ProviderResult<ChatCompletionResponse> {
        // 1. Get authentication token
        let token = state.token_manager.get_token().await?;

        // 2. Transform request (OpenAI → Vertex)
        let vertex_req = transform_request(request)?;

        // 3. Execute HTTP request
        let response = client.post(&url).json(&vertex_req).send().await?;

        // 4. Transform response (Vertex → OpenAI)
        let openai_response = transform_response(response).await?;

        Ok(openai_response)
    }
}
```

### 3. Request Transformation

Each provider transforms OpenAI-format requests to their native format:

```rust
fn transform_request(req: ChatCompletionRequest) -> Result<VertexRequest> {
    VertexRequest {
        contents: req.messages.into_iter()
            .map(|m| transform_message(m))
            .collect(),
        generation_config: GenerationConfig {
            temperature: req.temperature,
            max_output_tokens: req.max_tokens,
            // ...
        },
    }
}
```

### 4. Response Transformation

Providers transform their native responses back to OpenAI format:

```rust
fn transform_response(vertex_res: VertexResponse) -> ChatCompletionResponse {
    ChatCompletionResponse {
        id: format!("chatcmpl-{}", uuid),
        model: vertex_res.model,
        choices: vertex_res.candidates.into_iter()
            .map(|c| transform_choice(c))
            .collect(),
        // ...
    }
}
```

## Provider-Specific Details

### Anthropic Provider

**Architecture**: Uses Node.js bridge service

- Bridge handles Anthropic CLI authentication
- Bridge transforms formats
- Rust provider communicates via HTTP

**Request Flow**:

```text
Rust Provider → HTTP → Node.js Bridge → Anthropic CLI → Anthropic API
```

**Advantages**:

- Leverages existing Anthropic CLI tooling
- No Rust SDK dependency
- Isolated authentication

### Vertex Provider

**Architecture**: Direct API integration

- Uses Google Cloud authentication
- Supports API key and OAuth
- Direct HTTP requests to Vertex AI

**Request Flow**:

```text
Rust Provider → HTTP → Vertex AI API
```

**Advantages**:

- No intermediate service
- Lower latency
- Full control over requests

### OpenAI Provider

**Architecture**: Direct API with harvester

- Uses harvester service for tokens
- Handles Arkose challenges for GPT-4
- Direct HTTP requests to OpenAI

**Request Flow**:

```text
Rust Provider → HTTP → OpenAI API
Harvester → Token Management
```

## Error Handling

### ProviderError Types

```rust
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("Authentication failed: {0}")]
    Auth(String),

    #[error("Network error: {0}")]
    Network(String),

    #[error("Service unavailable: {0}")]
    Unavailable(String),

    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    #[error("Rate limited: {0}")]
    RateLimited(String),

    #[error("Internal error: {0}")]
    Internal(String),
}
```

### Error Mapping

Providers map their specific errors to `ProviderError`:

```rust
// Vertex provider example
if !response.status().is_success() {
    return Err(ProviderError::Unavailable(format!(
        "Vertex API Error (status: {}): {}",
        response.status(),
        response.text().await?
    )));
}
```

## Testing Pattern

### Unit Tests

```rust
#[tokio::test]
async fn should_execute_request_successfully() {
    // Given
    let mock_server = MockServer::start().await;
    let provider = VertexProvider::new();
    let state = create_test_state(mock_server.uri());

    // Mock API response
    Mock::given(method("POST"))
        .and(path("/v1beta/models/gemini-pro:generateContent"))
        .respond_with(ResponseTemplate::new(200).set_body_json(response))
        .mount(&mock_server)
        .await;

    // When
    let result = provider.execute(request, &state).await;

    // Then
    assert!(result.is_ok());
}
```

### Test Helpers

```rust
fn create_test_state(mock_server_uri: String) -> AppState {
    // Create test configuration
    // Initialize providers
    // Return AppState
}
```

## Adding a New Provider

### Steps

1. **Implement Provider Trait**:

   ```rust
   pub struct NewProvider;

   #[async_trait]
   impl Provider for NewProvider {
       // Implement all trait methods
   }
   ```

2. **Add Model Support**:

   ```rust
   fn supports_model(&self, model: &str) -> bool {
       model.starts_with("new-model-")
   }
   ```

3. **Implement Transformations**:
   - `transform_request`: OpenAI → Provider format
   - `transform_response`: Provider → OpenAI format

4. **Register Provider**:

   ```rust
   let registry = ProviderRegistry::new()
       .with_provider(Box::new(NewProvider::new()));
   ```

5. **Add Tests**:
   - Unit tests with HTTP mocking
   - Error handling tests
   - Edge case tests

## Best Practices

1. **Idempotent Transformations**: Transformations should be reversible
2. **Error Context**: Include model and request ID in errors
3. **Logging**: Log all provider operations with context
4. **Metrics**: Track provider-specific metrics
5. **Circuit Breaker**: Use circuit breaker for resilience
6. **Timeout Configuration**: Set appropriate timeouts per provider
7. **Request ID**: Generate unique request IDs for tracing

## Configuration

Providers can have provider-specific configuration:

```rust
pub struct VertexConfig {
    pub project_id: Option<String>,
    pub region: String,
    pub api_key: Option<String>,
    pub api_key_base_url: Option<String>, // For testing
    pub oauth_base_url: Option<String>,    // For testing
}
```

## Future Enhancements

1. **Provider Health Checks**: Monitor provider availability
2. **Load Balancing**: Distribute requests across provider instances
3. **Fallback Providers**: Automatic failover to backup providers
4. **Provider Metrics**: Detailed per-provider metrics
5. **Request Caching**: Cache responses per provider
