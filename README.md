# Vertex Bridge

[![CI](https://github.com/Lyther/FkLLMProxy/actions/workflows/ci.yml/badge.svg)](https://github.com/Lyther/FkLLMProxy/actions/workflows/ci.yml)

A high-performance Rust proxy that connects OpenAI-compatible clients (like Cursor, VSCode) to Google Gemini (via **Google AI Studio** or **Vertex AI**).

## 🚀 Quick Start

### 1. Prerequisites

You have **two** ways to authenticate:

- **Option A (Personal, Recommended)**:
  A **Google AI Studio API Key** for Gemini
  👉 This is created in **Google AI Studio**, **not** in “APIs & Services → Credentials”.

- **Option B (Enterprise / Production)**:
  A Google Cloud **Service Account** with the `Vertex AI User` role.

#### Where do I get the API Key exactly?

1. Open **Google AI Studio**: <https://aistudio.google.com/app/apikey>
2. Sign in with the Google account that owns your Gemini / Vertex trial.
3. Click **“Create API key”** and choose / confirm a Cloud Project.
4. Copy the key that looks like `AIzaSy...` — this is what you use as `GOOGLE_API_KEY`.

> If you have the \$300 Vertex trial: just select that same project when creating the API key.
> The key is still created in **AI Studio**, but billing/quotas go through that GCP project.

### 2. Configuration

#### Option A: Using API Key (Recommended for Individuals)

Set the environment variable:

```bash
export GOOGLE_API_KEY="AIzaSy..."
````

```env
# Required
GOOGLE_API_KEY=AIzaSy...

# Optional: Auth for the proxy itself
APP_AUTH__REQUIRE_AUTH=true
APP_AUTH__MASTER_KEY=sk-your-secret-key
```

When an API key is present, the bridge talks to
`generativelanguage.googleapis.com` (Google AI Studio Gemini API).

#### Option B: Using Service Account (Recommended for Production)

1. Create a service account on GCP and grant it the `Vertex AI User` role.
2. Download the JSON key.

   **Recommended**: Store in secure location outside project root:

   ```bash
   mkdir -p ~/.config/fkllmproxy
   mv service-account.json ~/.config/fkllmproxy/
   chmod 600 ~/.config/fkllmproxy/service-account.json
   ```

3. Set environment variable:

   ```bash
   export GOOGLE_APPLICATION_CREDENTIALS="$HOME/.config/fkllmproxy/service-account.json"
   export GOOGLE_CLOUD_PROJECT="your-project-id"
   ```

   > **Security**: See [Deployment Guide](docs/ops/deployment.md#security-best-practices) for credential security best practices.

In this mode the bridge talks to `aiplatform.googleapis.com` (Vertex AI).

### 3. Run

```bash
cargo run
```

Server starts at `http://127.0.0.1:4000` (default). Use `APP_SERVER__HOST=0.0.0.0` to bind to all interfaces.

### 4. Connect Cursor

1. Go to **Cursor Settings → Models**.
2. Add a custom model, e.g. `gemini-flash-latest` (or `gemini-pro-latest`).
3. Set **OpenAI Base URL** to: `http://localhost:4000/v1`.
4. Set **API Key** (the *client*-side key) to something like
   `sk-vertex-bridge-dev` (or whatever you configure in `.env` / config).

> This “API Key” is **just for your local bridge** and unrelated to the Google API key.
> The bridge itself uses `GOOGLE_API_KEY` or the service account credentials to talk to Google.

## 🧪 Testing

### Quick Test

```bash
curl -X POST http://localhost:4000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer sk-vertex-bridge-dev" \
  -d '{
    "model": "gemini-2.5-flash",
    "messages": [{"role": "user", "content": "Hello!"}]
  }'
```

### Test Suite

The project includes a comprehensive test suite with **25 passing tests** covering critical paths:

```bash
# Run smoke tests (< 2 minutes)
./scripts/test-smoke.sh

# Run all critical tests
./scripts/test-critical.sh

# Run specific test category
cargo test --test integration smoke_
cargo test --test integration auth_test
```

**Test Coverage**:

- ✅ Health endpoint
- ✅ Auth middleware (6 scenarios)
- ✅ Error handling (OpenAI-compatible format)
- ✅ Rate limiting
- ✅ Metrics endpoint
- ⚠️ Chat completions (2 E2E tests require credentials - auto-skip in local dev)

### Running E2E Tests Locally

E2E tests that require real API credentials are automatically skipped when credentials are missing:

```bash
# Tests will auto-skip if no credentials
cargo test --test integration -- --ignored

# With credentials (one of these):
export VERTEX_API_KEY="AIzaSy..."
# OR
export GOOGLE_APPLICATION_CREDENTIALS="/path/to/service-account.json"
export GOOGLE_CLOUD_PROJECT="your-project-id"

# Now E2E tests will run
cargo test --test integration -- --ignored
```

**Credential Detection**: Tests automatically detect credentials and skip gracefully in local development. In CI environments, set credentials as secrets to enable E2E tests.

### Test Environment Variables

| Variable | Purpose | Required For |
|----------|---------|--------------|
| `VERTEX_API_KEY` | Google AI Studio API key | E2E tests with API key auth |
| `GOOGLE_APPLICATION_CREDENTIALS` | Path to service account JSON | E2E tests with service account |
| `GOOGLE_CLOUD_PROJECT` | GCP project ID | E2E tests with service account |
| `VERTEX_REGION` | GCP region | E2E tests (default: `us-central1`) |
| `FORCE_E2E_TESTS` | Force E2E tests in CI even without credentials | CI (not recommended) |

See [Testing Guide](docs/dev/testing/guide.md) for full documentation.

## 📋 Supported Models

The bridge routes requests to providers based on model name prefixes. Here's how to check supported models:

### Model Routing

Models are automatically routed by prefix:

| Prefix | Provider | Examples |
|--------|----------|----------|
| `gemini-*` | Google Vertex AI | `gemini-3.0-pro`, `gemini-2.5-flash`, `gemini-2.5-pro`, `gemini-2.5-flash-lite` |
| `claude-*` | Anthropic CLI | `claude-3-5-sonnet`, `claude-3-opus`, `claude-3-haiku` |
| `gpt-*` | OpenAI (via Harvester) | `gpt-4`, `gpt-3.5-turbo`, `gpt-4-turbo` |
| `deepseek-*` | DeepSeek | ❌ **Not Implemented** - Only routing enum exists |
| `ollama-*` | Ollama | ❌ **Not Implemented** - Only routing enum exists |

**Default**: Unknown models default to Vertex AI (`gemini-*`).

### Checking Model Support

**Method 1: Test Request**

```bash
curl -X POST http://localhost:4000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer sk-vertex-bridge-dev" \
  -d '{
    "model": "gemini-2.5-flash",
    "messages": [{"role": "user", "content": "test"}],
    "max_tokens": 10
  }'
```

**Method 2: Check Provider Documentation**

- **Gemini**: See [Google Vertex AI Models Documentation](https://docs.cloud.google.com/vertex-ai/generative-ai/docs/models) or [Google AI Studio](https://aistudio.google.com/app/apikey)
- **Claude**: See [Anthropic Model Documentation](https://docs.anthropic.com/claude/docs/models-overview)
- **OpenAI**: Standard GPT models (`gpt-4`, `gpt-3.5-turbo`)

**Method 3: Review Code**
Model routing logic:

- **OpenAI models** (`gpt-*`): Handled first in `src/handlers/chat.rs` via `is_openai_model()` check
- **Other models**: Routed by prefix in `src/services/providers/mod.rs`:

```rust
pub fn route_provider(model: &str) -> Provider {
    if model.starts_with("gemini-") {
        Provider::Vertex
    } else if model.starts_with("claude-") {
        Provider::AnthropicCLI
    } else if model.starts_with("deepseek-") {
        Provider::DeepSeek  // ❌ NOT IMPLEMENTED - Will return error
    } else if model.starts_with("ollama-") {
        Provider::Ollama  // ❌ NOT IMPLEMENTED - Will return error
    } else {
        Provider::Vertex  // Default fallback
    }
}
```

### Common Model IDs

**Gemini (Vertex AI / Google AI Studio):**

> **Note**: Model IDs may vary. Always check the [official Google documentation](https://docs.cloud.google.com/vertex-ai/generative-ai/docs/models) for the exact model names available in your region.

**Current Models (as of November 2025):**

- `gemini-3.0-pro` - Latest advanced model for complex multimodal tasks and reasoning
- `gemini-3.0-deep-think` - Optimized for agentic workflows and autonomous coding (1M context)
- `gemini-2.5-pro` - High-capability model for complex reasoning and coding (1M context, adaptive thinking)
- `gemini-2.5-flash` - Fast and highly capable, balanced speed and price
- `gemini-2.5-flash-lite` - Cost-effective for high-throughput tasks (1M context, multimodal)
- `gemini-2.5-flash-image` - Optimized for rapid creative workflows with image generation

**Legacy Models (still supported):**

- `gemini-1.5-pro` - Previous generation high-capability model
- `gemini-1.5-flash` - Previous generation fast model
- `gemini-pro` - Standard model (may be deprecated)

**Claude (Anthropic):**

- `claude-3-5-sonnet` - Latest, best performance
- `claude-3-opus` - Highest capability
- `claude-3-sonnet` - Balanced
- `claude-3-haiku` - Fast, cost-effective

**OpenAI (requires Harvester):**

- `gpt-4` - Latest GPT-4
- `gpt-4-turbo` - GPT-4 Turbo
- `gpt-3.5-turbo` - Fast, cost-effective

## 📊 Metrics

The bridge exposes two metrics endpoints:

**JSON Metrics** (`/metrics`):

```bash
curl http://localhost:4000/metrics
```

Returns JSON with:

- `cache_hit_rate`: Token cache hit percentage
- `waf_block_rate`: WAF block percentage
- `arkose_solves`: Number of Arkose tokens generated
- `avg_arkose_solve_time_ms`: Average Arkose solve time
- `total_requests`: Total requests processed
- `success_rate`: Request success percentage

**Prometheus Metrics** (`/metrics/prometheus`):

```bash
curl http://localhost:4000/metrics/prometheus
```

Returns Prometheus-formatted metrics (text/plain) for scraping by monitoring systems.

## 📝 Environment Variables

| Variable | Required | Description |
|----------|----------|-------------|
| `GOOGLE_API_KEY` | Yes* | Google AI Studio API key |
| `GOOGLE_APPLICATION_CREDENTIALS` | Yes* | Path to service account JSON (alternative to API key) |
| `APP_SERVER__HOST` | No | Bind address (default: `127.0.0.1`) |
| `APP_SERVER__PORT` | No | Port (default: `4000`) |
| `APP_SERVER__MAX_REQUEST_SIZE` | No | Max request body size in bytes (default: `10485760` = 10MB) |
| `APP_AUTH__REQUIRE_AUTH` | No | Enable auth (default: `false`) |
| `APP_AUTH__MASTER_KEY` | No | API key for clients to use |
| `APP_VERTEX__PROJECT_ID` | No | GCP project ID (required if using service account) |
| `APP_VERTEX__REGION` | No | GCP region (default: `us-central1`) |
| `APP_VERTEX__API_KEY_BASE_URL` | No | Override API key base URL (for testing/mocking) |
| `APP_VERTEX__OAUTH_BASE_URL` | No | Override OAuth base URL (for testing/mocking) |
| `APP_LOG__LEVEL` | No | Log level (default: `info`) |
| `APP_OPENAI__HARVESTER_URL` | No | Harvester service URL (default: `http://localhost:3001`) |
| `APP_OPENAI__ACCESS_TOKEN_TTL_SECS` | No | Access token cache TTL in seconds (default: `3600`) |
| `APP_OPENAI__ARKOSE_TOKEN_TTL_SECS` | No | Arkose token cache TTL in seconds (default: `120`) |
| `APP_ANTHROPIC__BRIDGE_URL` | No | Anthropic bridge service URL (default: `http://localhost:4001`) |
| `APP_RATE_LIMIT__CAPACITY` | No | Rate limit bucket capacity (default: `100` requests) |
| `APP_RATE_LIMIT__REFILL_PER_SECOND` | No | Rate limit refill rate (default: `10` requests/second) |
| `APP_CIRCUIT_BREAKER__FAILURE_THRESHOLD` | No | Circuit breaker failure threshold (default: `10`) |
| `APP_CIRCUIT_BREAKER__TIMEOUT_SECS` | No | Circuit breaker timeout in seconds (default: `60`) |
| `APP_CIRCUIT_BREAKER__SUCCESS_THRESHOLD` | No | Circuit breaker success threshold (default: `3`) |
| `APP_CACHE__ENABLED` | No | Enable response caching (default: `false`) |
| `APP_CACHE__DEFAULT_TTL_SECS` | No | Cache TTL in seconds (default: `3600` = 1 hour) |
| `APP_OPENAI__TLS_FINGERPRINT_ENABLED` | No | Enable TLS fingerprinting for OpenAI (default: `false`) |
| `APP_OPENAI__TLS_FINGERPRINT_TARGET` | No | TLS fingerprint target: `chrome120` or `firefox120` (default: `chrome120`) |
| `APP_LOG__FORMAT` | No | Log format: `json` or `pretty` (default: `pretty`) |
| `VERTEX_API_KEY` | No* | Google AI Studio API key (for E2E tests) |
| `VERTEX_REGION` | No | GCP region for Vertex AI (default: `us-central1`, for E2E tests) |

\* One of `GOOGLE_API_KEY` or `GOOGLE_APPLICATION_CREDENTIALS` is required for runtime.
\*\* Test-specific variables (`VERTEX_API_KEY`, etc.) are only needed for E2E tests.

> **Note**: For CI/CD setup, see [Deployment Guide](docs/ops/deployment.md) for environment configuration.

## 🤖 OpenAI Support (Experimental)

The bridge now supports OpenAI models (`gpt-4`, `gpt-3.5-turbo`) via the ChatGPT Web Interface.

### Prerequisites

1. **Harvester Service**: A separate TypeScript service manages browser sessions and token extraction.
2. **Valid ChatGPT Session**: You need to be logged into ChatGPT in the browser.

### Setup

1. **Start the Harvester**:

   ```bash
   cd harvester
   npm install
   npm run dev
   ```

   The Harvester runs on `http://localhost:3001` by default.

2. **Configure the Bridge**:

   ```env
   APP_OPENAI__HARVESTER_URL=http://localhost:3001
   ```

3. **Use OpenAI Models**:
   Point Cursor to the same base URL (`http://localhost:4000/v1`) and use models like:
   - `gpt-4`
   - `gpt-3.5-turbo`

### Docker Setup

All services can run via Docker Compose:

```bash
docker-compose up -d
```

This starts:

- **vertex-bridge** (Rust proxy) on port 4000
- **harvester** (OpenAI session manager) on port 3001
- **anthropic-bridge** (Anthropic CLI bridge) on port 4001

**Testing the Setup:**

After starting services, verify everything works:

```bash
# Run the Docker Compose test script
./scripts/test-docker-compose.sh
```

This script:

- Verifies all services start correctly
- Checks health endpoints
- Validates service connectivity
- Shows recent logs from each service

**Note for Anthropic Bridge**: The Anthropic CLI (`claude`) must be authenticated before use. You can:

1. Authenticate inside the container: `docker exec -it anthropic-bridge claude login`
2. Mount your host's CLI config (uncomment volumes in docker-compose.yml)

### Limitations

- **TLS Fingerprinting**: Currently using standard `reqwest` (WAF may block). TLS impersonation requires `reqwest-impersonate` which needs manual setup.
- **Session Management**: Requires manual login in browser initially. Cookies are persisted for session recovery.
- **Arkose Tokens**: Required for GPT-4, generated automatically via browser automation.

## 🦙 Anthropic Support (Experimental)

The bridge supports Anthropic Claude models via the official CLI.

### Prerequisites

1. **Anthropic CLI**: Install and authenticate the official CLI:

   ```bash
   npm install -g @anthropic-ai/claude-code
   claude login
   ```

2. **Bridge Service**: A separate TypeScript service bridges CLI stdio to HTTP.

### Setup

1. **Start the Bridge Service**:

   ```bash
   cd bridge
   npm install
   npm run dev
   ```

   The Bridge runs on `http://localhost:4001` by default.

2. **Configure the Proxy** (optional):

   The proxy defaults to `http://localhost:4001` for the Anthropic bridge. To use a different URL, set:

   ```env
   APP_ANTHROPIC__BRIDGE_URL=http://localhost:4001
   ```

3. **Use Claude Models**:
   Point Cursor to the same base URL (`http://localhost:4000/v1`) and use models like:
   - `claude-3-5-sonnet`
   - `claude-3-opus`
   - `claude-3-sonnet`
   - `claude-3-haiku`

### How It Works

- The Rust proxy routes `claude-*` models to the Anthropic bridge service
- The bridge service spawns `claude -p` CLI command with the prompt
- CLI output (with ANSI codes stripped) is converted to OpenAI-format SSE chunks
- Uses your Pro subscription quota directly (0% ban risk)

### Limitations

- **Context Window**: Full conversation history is sent each time (CLI is stateless)
- **Token Consumption**: Slightly higher than API mode due to history resending
- **Requires CLI**: Must have `claude` command available in PATH

## 🔒 Security & Credentials

### Credential Management

**Current Status**: Credential files are secured in `.gitignore` and not tracked.

**Best Practices**:

- Store credentials outside project root (recommended: `~/.config/fkllmproxy/`)
- Set file permissions to `600` (read/write owner only)
- Use environment variables, not hardcoded paths
- Rotate credentials regularly (every 90 days)

**Quick Migration**:

```bash
mkdir -p ~/.config/fkllmproxy
mv service-account.json ~/.config/fkllmproxy/
chmod 600 ~/.config/fkllmproxy/service-account.json
export GOOGLE_APPLICATION_CREDENTIALS="$HOME/.config/fkllmproxy/service-account.json"
```

**Documentation**:

- [Deployment Guide](docs/ops/deployment.md) - Complete deployment guide with security best practices
- [Operational Runbook](docs/ops/runbook.md) - Day-to-day operations and troubleshooting

### Production Deployment

For production, use:

- Secret management systems (Kubernetes Secrets, Vault, etc.)
- Systemd environment variables
- Separate credentials per environment
- Regular credential rotation

See [Deployment Guide](docs/ops/deployment.md) for detailed instructions.

## 🏗️ Architecture

- **Rust / Axum**: High-performance async web server.
- **Dual Auth Mode**: Supports both **Google AI Studio (API Key)** and **Vertex AI (Service Account)**.
- **Transformer**: Maps OpenAI-compatible JSON to Gemini / Vertex JSON on the fly.
- **Multi-Provider Support**:
  - **Vertex AI**: Direct HTTP integration
  - **OpenAI**: Split-process design (Rust Enforcer + TypeScript Harvester)
  - **Anthropic**: Split-process design (Rust Enforcer + TypeScript Bridge)
- **Production Ready**: Kubernetes manifests, monitoring, security audit tools, performance testing
- **Observability**: Prometheus metrics, structured logging, health checks

## 🚀 Production Features

### Deployment Options

- **Docker Compose**: `docker-compose.prod.yml` for production
- **Kubernetes**: Complete manifests in `k8s/` directory
- **Monitoring**: Prometheus metrics, Grafana dashboards, alerting rules
- **Security**: Automated security audit scripts, dependency scanning

### New Features

- **TLS Fingerprinting**: Configuration structure for OpenAI WAF bypass (see [ADR 005](docs/dev/adr/005-tls-fingerprinting.md))
- **Performance Testing**: Load testing scripts in `scripts/load-test.sh`
- **Security Audit**: Automated security scanning in `scripts/security-audit.sh`

See [Deployment Guide](docs/ops/deployment.md) and [Monitoring Guide](docs/ops/monitoring.md) for details.
