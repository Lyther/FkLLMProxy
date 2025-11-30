# Utoipa OpenAPI Auto-Generation Setup

This document describes the setup for auto-generating OpenAPI specifications using `utoipa`.

## Status

**Current**: Dependencies added, ready for implementation
**Next Steps**: Annotate handlers and models with utoipa attributes

## Dependencies

Added to `Cargo.toml`:

- `utoipa = { version = "5.4", features = ["axum_extras", "chrono"] }`
- `utoipa-swagger-ui = { version = "9.0", features = ["axum"] }`

## Implementation Plan

### 1. Annotate Models

Add `#[derive(ToSchema)]` to request/response models:

```rust
use utoipa::ToSchema;

#[derive(Serialize, Deserialize, ToSchema)]
pub struct ChatCompletionRequest {
    // ...
}
```

### 2. Annotate Handlers

Add OpenAPI operation attributes:

```rust
#[utoipa::path(
    post,
    path = "/v1/chat/completions",
    request_body = ChatCompletionRequest,
    responses(
        (status = 200, description = "Success", body = ChatCompletionResponse),
        (status = 400, description = "Bad Request"),
        (status = 401, description = "Unauthorized"),
    ),
    tag = "Chat"
)]
pub async fn chat_completions(...) { ... }
```

### 3. Generate OpenAPI Spec

Add to `main.rs`:

```rust
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

#[derive(OpenApi)]
#[openapi(
    paths(handlers::chat::chat_completions, handlers::health::health_check),
    components(schemas(ChatCompletionRequest, ChatCompletionResponse)),
    tags(
        (name = "Chat", description = "Chat completion endpoints"),
        (name = "Health", description = "Health check endpoints")
    ),
)]
struct ApiDoc;

// In router setup:
let app = Router::new()
    .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
    // ... other routes
```

### 4. CI Integration

Add to `.github/workflows/ci.yml`:

```yaml
- name: Validate OpenAPI spec
  run: |
    cargo run --bin generate-openapi > docs/dev/api/openapi.yaml
    git diff --exit-code docs/dev/api/openapi.yaml
```

## Benefits

- **Single Source of Truth**: OpenAPI spec generated from code
- **Always Up-to-Date**: Spec matches implementation automatically
- **Interactive Docs**: Swagger UI for testing
- **Type Safety**: Compile-time validation of API contracts

## References

- [Utoipa Documentation](https://docs.rs/utoipa/)
- [Utoipa Examples](https://github.com/juhaku/utoipa/tree/master/examples)
