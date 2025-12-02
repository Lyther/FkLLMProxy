# Scripts

Build, test, and deployment scripts for FkLLMProxy.

## Build Scripts

### `build-artifact.sh`

**IMMUTABLE REALITY.** Build multi-arch container images with full traceability.

Builds all three services (vertex-bridge, harvester, anthropic-bridge) as multi-architecture Docker images with OCI labels for traceability.

**Usage:**

```bash
# Build locally (single arch, no push)
./scripts/build-artifact.sh

# Build with semantic version tag
./scripts/build-artifact.sh v1.2.3

# Build and push to registry
PUSH=true ./scripts/build-artifact.sh v1.2.3

# Custom registry
REGISTRY=docker.io REPO=myuser/fkllmproxy PUSH=true ./scripts/build-artifact.sh
```

**Features:**

- **Dirty Check**: Aborts if git working directory is dirty (untraceable builds forbidden)
- **Multi-Arch**: Builds for `linux/amd64` and `linux/arm64`
- **OCI Labels**: Injects source URL, git revision, build date for traceability
- **Tagging**: Primary tag is `sha-<commit>`, optional semantic version

**Environment Variables:**

| Variable | Default | Description |
|----------|---------|-------------|
| `REGISTRY` | `ghcr.io` | Container registry |
| `REPO` | `lyther/fkllmproxy` | Repository path |
| `PLATFORMS` | `linux/amd64,linux/arm64` | Target platforms |
| `PUSH` | `false` | Push to registry |

**Images Built:**

- `${REGISTRY}/${REPO}/vertex-bridge:sha-<commit>`
- `${REGISTRY}/${REPO}/harvester:sha-<commit>`
- `${REGISTRY}/${REPO}/anthropic-bridge:sha-<commit>`

---

## Test Scripts

Scripts for testing the FkLLMProxy server.

## Prerequisites

- Server must be running (`cargo run`) - for proxy test scripts
- Environment variables configured (see `.env.example`)
- Optional: `jq` for better JSON formatting
- For `test-verify.sh`: `cargo`, `bc` (for coverage calculations), `cargo-tarpaulin` (auto-installed if missing)

## Scripts

### `prove.sh`

**TALK IS CHEAP. SHOW ME THE RUNNING SYSTEM.** End-to-end proof that the full system works.

This script implements a strict proving protocol:

1. **Cold Boot**: Clean build from scratch (`--no-cache`), start all containers
2. **Probing**: Test health endpoints and critical workflows (chat completions)
3. **Deep Dive**: Verify service connectivity, analyze logs for errors
4. **Evidence Locker**: Captures all proof (logs, responses, metrics) for inspection

**Usage:**

```bash
# Run full proof (builds, starts, tests, shows evidence)
./scripts/prove.sh

# Keep services running after proof
KEEP_RUNNING=true ./scripts/prove.sh

# Auto-shutdown after proof
KEEP_RUNNING=false ./scripts/prove.sh
```

**Output Format:**

```text
=== Evidence Locker ===

Status Summary:
  Services: 🟢 ONLINE
  Health Checks: ✅ PASSED
  Chat Completions: ✅ PASSED
  Log Analysis: ✅ CLEAN

Endpoints Verified:
  - GET /health: 200 OK
  - POST /v1/chat/completions: 200 Created (1234ms)

✅ VERDICT: SYSTEM PROVEN
```

**Features:**

- **No Mocks**: Tests against real Docker containers
- **Clean Build**: Always rebuilds from scratch (no cache)
- **Smart Waiting**: Polls health endpoints instead of sleeping
- **Evidence Capture**: Saves all responses/logs for inspection
- **Port Conflict Detection**: Warns if ports are in use
- **Log Analysis**: Searches for errors/warnings automatically

### `test-verify.sh`

**THE TRUTH SERUM.** Comprehensive test suite execution with environment checks, coverage tracking, and flake detection.

This script implements a strict testing protocol:

1. **Environment Check**: Validates test environment, prevents running against production
2. **Pyramid Execution**: Unit tests first (< 10ms, no network/DB), then integration tests
3. **Coverage Audit**: Tracks code coverage with ratchet rule (fails if coverage decreases)
4. **Flake Detection**: Automatically retries failed tests 3x to detect flaky tests

**Usage:**

```bash
# Run full test suite
./scripts/test-verify.sh

# Or via alias (if configured)
make test-verify
```

**Output Format:**

```text
=== Test Execution Report ===
Unit: ✅ 452 Passing (320ms)
Integration: ✅ 48 Passing (12s)
Flaky: ⚠️ auth_test::test_name passed on attempt #2
Coverage: 84.5% (⬆️ +0.2% - Good job)
Verdict: PASS
```

**Features:**

- **Fail Fast**: Unit test failures stop execution immediately
- **Coverage Ratchet**: Compares against main/master branch, fails if coverage decreases
- **Flake Detection**: Marks tests that pass on retry as flaky (warning, not failure)
- **Slow Test Detection**: Warns about unit tests taking >1s
- **Production Safety**: Aborts if production URLs/keys detected in non-CI environment

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
