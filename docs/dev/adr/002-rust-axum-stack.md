# ADR 002: Rust + Axum Web Framework

**Status**: Accepted
**Date**: 2024
**Deciders**: Architecture Team

## Context

We needed a high-performance, type-safe web server that could:

- Handle OpenAI-compatible HTTP/SSE endpoints
- Support async streaming responses
- Provide excellent error handling
- Have minimal runtime overhead
- Support production deployment patterns

## Decision

We chose:

- **Rust** as the primary language for type safety and performance
- **Axum 0.7** as the web framework (async/await, tower middleware)
- **Tokio** as the async runtime

## Consequences

### Positive

- **Performance**: Near-zero overhead async runtime
- **Type Safety**: Compile-time guarantees reduce runtime errors
- **Memory Safety**: No garbage collection, predictable memory usage
- **Ecosystem**: Strong async HTTP ecosystem in Rust
- **Deployment**: Single binary deployment, no runtime dependencies

### Negative

- **Learning Curve**: Rust has a steeper learning curve
- **Compile Time**: Slower compilation compared to interpreted languages
- **Ecosystem**: Smaller ecosystem compared to Node.js/Python for some tasks

## Alternatives Considered

- **Node.js/Express**: Faster development, but higher memory usage and runtime overhead
- **Go**: Good performance, but less type safety and async model differences
- **Python/FastAPI**: Easy development, but performance and memory overhead concerns

## References

- `src/main.rs` - Main server setup
- `Cargo.toml` - Dependencies
- `docs/dev/architecture/system-overview.md` - Architecture overview
