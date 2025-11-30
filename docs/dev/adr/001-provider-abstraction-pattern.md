# ADR 001: Provider Abstraction Pattern

**Status**: Accepted
**Date**: 2024
**Deciders**: Architecture Team

## Context

The system needs to support multiple LLM providers (Vertex AI, Anthropic, OpenAI) with different API formats, authentication methods, and capabilities. We needed a way to:

- Support multiple providers without code duplication
- Allow new providers to be added easily
- Maintain a unified OpenAI-compatible interface
- Handle provider-specific transformations transparently

## Decision

We implemented a trait-based provider abstraction pattern where:

1. All providers implement a common `Provider` trait
2. A `ProviderRegistry` routes requests based on model name prefix
3. Each provider handles its own request/response transformation
4. Providers are discovered and registered at startup

## Consequences

### Positive

- **Extensibility**: New providers can be added by implementing the trait
- **Maintainability**: Provider-specific code is isolated
- **Testability**: Providers can be mocked easily
- **Consistency**: All providers expose the same interface

### Negative

- **Initial Complexity**: Requires trait design and registry setup
- **Overhead**: Small runtime cost for trait dispatch (minimal in Rust)

## Implementation

See `docs/dev/architecture/provider-pattern.md` for detailed implementation guide.

## References

- `src/services/providers/mod.rs` - Provider trait and registry
- `src/services/providers/vertex.rs` - Vertex AI implementation
- `src/services/providers/anthropic.rs` - Anthropic implementation
