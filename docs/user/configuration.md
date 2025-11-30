# Provider Configuration Guide

This guide explains how to configure and use different LLM providers with the FkLLMProxy.

## Supported Providers

| Provider | Model Prefix | Authentication | Status |
|----------|-------------|----------------|--------|
| Google Vertex AI | `gemini-*` | API Key or GCP Service Account | ✅ Implemented |
| Anthropic CLI | `claude-*` | OAuth (via `claude login`) | ✅ Implemented |
| DeepSeek | `deepseek-*` | API Key | 🚧 Planned |
| Ollama | `ollama-*` | Local instance | 🚧 Planned |

## Google Vertex AI Configuration

### Option 1: API Key (Gemini API)

**Environment Variables:**

```bash
export GOOGLE_API_KEY="your-gemini-api-key"
```

**Obtaining API Key:**

1. Go to [Google AI Studio](https://makersuite.google.com/app/apikey)
2. Create a new API key
3. Set as `GOOGLE_API_KEY` environment variable

**Supported Models:**

- `gemini-2.0-flash`
- `gemini-1.5-pro`
- `gemini-1.5-flash`

### Option 2: GCP Service Account (Vertex AI)

**Environment Variables:**

```bash
export GOOGLE_APPLICATION_CREDENTIALS="/path/to/service-account.json"
export APP_VERTEX__PROJECT_ID="your-gcp-project-id"
export APP_VERTEX__REGION="us-central1"
```

**Setup Steps:**

1. Create a GCP project or use existing one
2. Enable Vertex AI API
3. Create a service account with Vertex AI permissions
4. Download service account JSON key
5. Set environment variables

**Benefits:**

- Access to all Gemini models
- Enterprise-grade security
- Better rate limits
- Official Google support

## Anthropic CLI Configuration

### Prerequisites

**Install Claude CLI:**

```bash
npm install -g @anthropic-ai/claude-code
```

**Authenticate:**

```bash
claude login
```

This will open a browser for OAuth authentication with Anthropic.

### Configuration

**Environment Variables:**

```bash
export ANTHROPIC_CLI_PATH="/usr/local/bin/claude"  # Optional, auto-detected
```

**Supported Models:**

- `claude-3-5-sonnet`
- `claude-3-opus`
- `claude-3-haiku`
- `claude-3-sonnet`

### Features

- **Zero Account Ban Risk**: Uses official Anthropic binary
- **Pro Quota Access**: Direct access to your Pro subscription
- **Offline Capable**: Works without internet after authentication
- **Context Preservation**: Full conversation history maintained

## DeepSeek Configuration (Planned)

### Environment Variables

```bash
export DEEPSEEK_API_KEY="your-deepseek-api-key"
export DEEPSEEK_BASE_URL="https://api.deepseek.com"
```

### Supported Models

- `deepseek-chat`
- `deepseek-coder`

## Ollama Configuration (Planned)

### Prerequisites

```bash
# Install Ollama
curl -fsSL https://ollama.ai/install.sh | sh

# Pull models
ollama pull llama2
ollama pull codellama
```

### Environment Variables

```bash
export OLLAMA_BASE_URL="http://localhost:11434"
```

### Supported Models

- `ollama/llama2`
- `ollama/codellama`
- `ollama/mistral`

## Provider Routing Logic

The proxy automatically routes requests based on model name prefixes:

```rust
fn route_provider(model: &str) -> Provider {
    match model {
        m if m.starts_with("gemini-") => Provider::Vertex,
        m if m.starts_with("claude-") => Provider::AnthropicCLI,
        m if m.starts_with("deepseek-") => Provider::DeepSeek,
        m if m.starts_with("ollama/") => Provider::Ollama,
        _ => Provider::Vertex, // Default fallback
    }
}
```

## Circuit Breaker Configuration

Each provider has automatic circuit breaker protection:

```yaml
# Default settings
failure_threshold: 5    # Failures before opening circuit
success_threshold: 3    # Successes before closing circuit
timeout: 60s           # Half-open timeout
```

**Health Check:**

```bash
curl http://localhost:4000/health
```

Response includes provider status:

```json
{
  "providers": {
    "Vertex": {
      "state": "Closed",
      "failure_count": 0,
      "success_count": 10
    },
    "AnthropicCLI": {
      "state": "Closed",
      "failure_count": 0,
      "success_count": 5
    }
  }
}
```

## Rate Limiting

**Default Configuration:**

- 60 requests per minute per IP address
- Token bucket algorithm
- Automatic cleanup

**Customization:**

```rust
// In code: src/middleware/rate_limit.rs
let requests_per_minute = 60; // Adjust as needed
```

## Timeout Configuration

**Provider-specific timeouts:**

| Provider | Timeout | Purpose |
|----------|---------|---------|
| Vertex AI | 30s | HTTP API calls |
| Anthropic CLI | 120s | CLI process execution |
| DeepSeek | 60s | API calls |
| Ollama | 300s | Local model inference |

## Environment Variables Reference

### Core Configuration

```bash
# Server
APP_SERVER__HOST=127.0.0.1
APP_SERVER__PORT=4000

# Authentication
APP_AUTH__REQUIRE_AUTH=false
APP_AUTH__MASTER_KEY="your-secret-key"

# Logging
LOG_LEVEL=info
```

### Provider Configuration

```bash
# Google Vertex AI
GOOGLE_API_KEY="your-api-key"
GOOGLE_APPLICATION_CREDENTIALS="/path/to/service-account.json"
APP_VERTEX__PROJECT_ID="your-project"
APP_VERTEX__REGION="us-central1"

# Anthropic CLI
ANTHROPIC_CLI_PATH="/usr/local/bin/claude"

# DeepSeek (future)
DEEPSEEK_API_KEY="your-api-key"
DEEPSEEK_BASE_URL="https://api.deepseek.com"

# Ollama (future)
OLLAMA_BASE_URL="http://localhost:11434"
```

### Circuit Breaker (Code Configuration)

```rust
// src/services/circuit_breaker.rs
let config = CircuitBreaker::new(
    5,  // failure_threshold
    3,  // success_threshold
    Duration::from_secs(60),  // timeout
    Duration::from_secs(30),  // half_open_timeout
);
```

## Monitoring and Debugging

### Health Endpoints

**Main Proxy:**

```bash
curl http://localhost:4000/health
```

**Anthropic Bridge:**

```bash
curl http://localhost:4001/health
```

### Logs

**View logs:**

```bash
# Docker
docker-compose logs proxy
docker-compose logs anthropic-bridge

# Direct
tail -f /var/log/llm-proxy.log
```

### Circuit Breaker States

- **Closed**: Normal operation, requests allowed
- **Open**: Failing, requests blocked
- **HalfOpen**: Testing recovery, limited requests allowed

### Troubleshooting Provider Issues

**Vertex AI Issues:**

```bash
# Check API key
echo $GOOGLE_API_KEY

# Test API directly
curl "https://generativelanguage.googleapis.com/v1beta/models/gemini-pro:generateContent?key=$GOOGLE_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"contents":[{"parts":[{"text":"Hello"}]}]}'
```

**Anthropic CLI Issues:**

```bash
# Check CLI installation
which claude

# Test CLI directly
echo "Hello" | claude -p

# Check login status
claude --help | grep login
```

## Performance Optimization

### Connection Pooling

- HTTP clients automatically pool connections
- Configurable via environment variables

### Concurrent Requests

- Tokio runtime handles async operations
- Default thread pool based on CPU cores

### Memory Usage

- Circuit breaker uses DashMap for thread-safe state
- Rate limiter uses in-memory storage
- No persistent state (resets on restart)

## Security Considerations

### API Keys

- Never commit keys to version control
- Use environment variables or secret management
- Rotate keys regularly

### Network Security

- HTTPS-only communication
- Validate SSL certificates
- Use internal networking for bridge communication

### Access Control

- Optional authentication middleware
- IP-based rate limiting
- Request validation

## Future Provider Additions

### Adding a New Provider

1. **Implement LLMProvider trait:**

```rust
pub struct NewProvider;

#[async_trait]
impl LLMProvider for NewProvider {
    async fn execute(&self, request: ChatCompletionRequest, state: &AppState) -> ProviderResult<ChatCompletionResponse> {
        // Implementation
        todo!()
    }

    fn provider_type(&self) -> Provider {
        Provider::NewProvider
    }

    fn supports_model(&self, model: &str) -> bool {
        model.starts_with("new-")
    }
}
```

2. **Add to ProviderRegistry:**

```rust
providers.insert(
    Provider::NewProvider,
    Arc::new(NewProvider::new()) as Arc<dyn LLMProvider>,
);
```

3. **Update routing:**

```rust
m if m.starts_with("new-") => Provider::NewProvider,
```

4. **Add configuration section to this guide**

This ensures consistent behavior across all providers with automatic circuit breaker and rate limiting protection.
