# ADR 003: OpenAI Format Compatibility

**Status**: Accepted
**Date**: 2024
**Deciders**: Architecture Team

## Context

The proxy needs to work with existing OpenAI-compatible clients (Cursor IDE, VSCode extensions, etc.) without modification. These clients expect:

- OpenAI API endpoint format (`/v1/chat/completions`)
- OpenAI request/response JSON structure
- SSE streaming format
- OpenAI error response format

## Decision

We maintain OpenAI format compatibility at the API boundary:

- Accept OpenAI-format requests
- Transform internally to provider-native format
- Transform provider responses back to OpenAI format
- Preserve OpenAI error codes and messages

## Consequences

### Positive

- **Client Compatibility**: Works with any OpenAI-compatible client
- **No Client Changes**: Existing tools work without modification
- **Provider Agnostic**: Clients don't need to know about providers

### Negative

- **Transformation Overhead**: Small overhead for format conversion
- **Limitation**: Some provider features may not map perfectly to OpenAI format

## Implementation

- Request transformation: `src/services/transformer.rs`
- Response transformation: Provider implementations
- Error mapping: `src/openai/errors.rs`

## References

- `src/models/openai.rs` - OpenAI format models
- `docs/dev/architecture/system-overview.md` - Data flow diagrams
