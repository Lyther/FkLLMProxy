# Test Scripts

Scripts for testing the FkLLMProxy server.

## Prerequisites

- Server must be running (`cargo run`)
- Environment variables configured (see `.env.example`)
- Optional: `jq` for better JSON formatting

## Scripts

### `test-proxy.sh`

Main test script that:

1. Lists all supported model IDs
2. Tests server health
3. Sends a test request to a selected model

**Usage:**

```bash
# Use default model (gemini-2.5-flash)
./scripts/test-proxy.sh

# Test a specific model
TEST_MODEL=gemini-1.5-pro ./scripts/test-proxy.sh

# Custom server address
APP_SERVER__HOST=0.0.0.0 APP_SERVER__PORT=4000 ./scripts/test-proxy.sh
```

**Environment Variables:**

- `TEST_MODEL` - Model to test (default: `gemini-2.5-flash`)
- `APP_SERVER__HOST` - Server host (default: `127.0.0.1`)
- `APP_SERVER__PORT` - Server port (default: `4000`)
- `APP_AUTH__REQUIRE_AUTH` - Whether auth is required (default: `false`)
- `APP_AUTH__MASTER_KEY` - Master key if auth is enabled

### `test-proxy-stream.sh`

Tests streaming chat completions.

**Usage:**

```bash
./scripts/test-proxy-stream.sh

# Test specific model
TEST_MODEL=gemini-2.5-pro ./scripts/test-proxy-stream.sh
```

## Supported Models

### Gemini (Vertex AI)

- `gemini-3.0-pro`
- `gemini-3.0-deep-think`
- `gemini-2.5-pro`
- `gemini-2.5-flash` (recommended for testing)
- `gemini-2.5-flash-lite`
- `gemini-2.5-flash-image`
- `gemini-1.5-pro`
- `gemini-1.5-flash`
- `gemini-pro`

### Claude (Anthropic CLI)

- `claude-3-5-sonnet`
- `claude-3-opus`
- `claude-3-sonnet`
- `claude-3-haiku`

### OpenAI (requires Harvester)

- `gpt-4`
- `gpt-4-turbo`
- `gpt-3.5-turbo`

## Examples

```bash
# Test Gemini Flash
TEST_MODEL=gemini-2.5-flash ./scripts/test-proxy.sh

# Test Claude
TEST_MODEL=claude-3-haiku ./scripts/test-proxy.sh

# Test with streaming
TEST_MODEL=gemini-2.5-flash ./scripts/test-proxy-stream.sh
```
