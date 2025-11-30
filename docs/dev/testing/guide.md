# Testing Guide

## Overview

We use a layered testing strategy to ensure reliability without sacrificing velocity.

1. **Unit Tests** (`cargo test --lib`): Fast, isolated logic verification.
2. **Integration Tests** (`cargo test --test integration`): Verifies component interaction.
3. **End-to-End (E2E) Tests** (`scripts/test-smoke.sh`): Verifies the full system against real/mock services.

## Running Tests

### Unit Tests

```bash
cargo test --lib
```

### Integration Tests

```bash
# Run all integration tests
cargo test --test integration

# Run specific category
cargo test --test integration smoke_
cargo test --test integration auth_
```

### Smoke Tests (Script)

```bash
./scripts/test-smoke.sh
```

## Test Structure

### Provider Tests (`src/services/providers/*`)

Each provider has a dedicated test suite using `wiremock` to simulate upstream APIs.

- **Anthropic**: `src/services/providers/anthropic.rs` (Tests `AnthropicBridgeProvider`)
- **Vertex**: `src/services/providers/vertex.rs` (Tests `VertexProvider`)

**Pattern:**

```rust
#[tokio::test]
async fn should_handle_scenario() {
    let mock_server = MockServer::start().await;
    // ... setup mock ...
    let result = provider.execute(request, &state).await;
    assert!(result.is_ok());
}
```

### Integration Tests (`tests/integration/*`)

- `health_test.rs`: Basic health check.
- `auth_test.rs`: Middleware verification.
- `chat_test.rs`: Full chat flow (requires credentials for some tests).
- `metrics_test.rs`: Observability endpoints.

## Status (November 2025)

| Suite | Status | Count | Notes |
|:---|:---|:---|:---|
| **Unit** | ✅ PASS | 48 | 100% coverage of core logic |
| **Anthropic** | ✅ PASS | 24 | Full edge case coverage |
| **Vertex** | ✅ PASS | 8 | Configurable URLs implemented |
| **Integration**| ✅ PASS | 26 | Critical paths covered |

## writing New Tests

1. **Isolate**: Can this be a unit test? If yes, put it in `src/`.
2. **Mock**: Do not hit external APIs in unit tests. Use `wiremock`.
3. **credentials**: If testing real APIs, use `#[ignore]` unless specific env vars are set.
