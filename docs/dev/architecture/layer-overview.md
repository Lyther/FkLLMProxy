# Unified Single-Port Architecture

FkLLMProxy provides a unified OpenAI-compatible API gateway that routes requests to multiple LLM providers through a single base URL and API key. All providers are accessible through port 4000 with automatic model-based routing.

## Architecture Overview

```text
┌─────────────┐
│   Client    │ (Cursor, curl, Python SDK, etc.)
│             │
│ Base URL:   │
│ :4000/v1    │
│ Key: master │
└──────┬──────┘
       │
       ▼
┌─────────────────────────────────────┐
│   FkLLMProxy (Port 4000)            │
│   - Authentication (master_key)     │
│   - Model Routing (by prefix)       │
│   - Request/Response Transformation │
└──────┬──────────────────────────────┘
       │
       ├──► claude-* ──► Anthropic Bridge (internal :4001)
       │
       ├──► gpt-* ─────► OpenAI Harvester (internal :3001)
       │
       └──► gemini-* ──► Vertex AI (direct API call)
```

## Key Features

- **Single Base URL**: `http://localhost:4000/v1`
- **Single API Key**: Configured via `APP_AUTH__MASTER_KEY` environment variable
- **Automatic Routing**: Model name prefix determines provider
- **OpenAI-Compatible**: Standard `/v1/chat/completions` endpoint
- **No Middleware Required**: Clients connect directly (no LiteLLM needed)

## Model Routing

Requests are automatically routed to the appropriate provider based on the model name prefix:

| Model Prefix | Provider | Internal Service | Examples |
|--------------|----------|------------------|----------|
| `claude-*` | Anthropic CLI | `anthropic-bridge:4001` | `claude-3-5-sonnet`, `claude-3-opus`, `claude-3-haiku` |
| `gpt-*` | OpenAI (Web) | `harvester:3001` | `gpt-4`, `gpt-3.5-turbo`, `gpt-4-turbo` |
| `gemini-*` | Vertex AI | Direct API | `gemini-3.0-pro`, `gemini-2.5-flash`, `gemini-2.5-pro` |
| Other | Vertex AI (default) | Direct API | Unknown models default to Vertex |

**Routing Logic** (`src/services/providers/mod.rs`):

```rust
// Provider routing uses ProviderRegistry which checks model prefixes:
impl ProviderRegistry {
    pub fn route_by_model(&self, model: &str) -> Option<Arc<dyn LLMProvider>> {
        // Returns Some(provider) if model prefix matches a registered provider
        // - "gemini-*" → Vertex AI
        // - "claude-*" → Anthropic (via bridge)
        // - "gpt-*" → Handled separately in chat.rs before reaching registry
        // Returns None for unknown models
        self.providers.iter()
            .find(|(_, provider)| provider.supports_model(model))
            .map(|(_, provider)| provider.clone())
    }
}
```

## Client Configuration

### Cursor IDE

1. Open Cursor Settings → Features → AI
2. Configure Custom Model:
   - **Base URL**: `http://localhost:4000/v1`
   - **API Key**: `sk-vertex-bridge-dev` (or your `APP_AUTH__MASTER_KEY`)
   - **Model**: Enter any supported model (e.g., `claude-3-5-sonnet`, `gpt-4`, `gemini-2.5-pro`)

### curl Example

```bash
curl -X POST http://localhost:4000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer sk-vertex-bridge-dev" \
  -d '{
    "model": "claude-3-5-sonnet",
    "messages": [
      {"role": "user", "content": "Hello, world!"}
    ],
    "max_tokens": 100
  }'
```

### Python SDK Example

```python
from openai import OpenAI

client = OpenAI(
    base_url="http://localhost:4000/v1",
    api_key="sk-vertex-bridge-dev"
)

response = client.chat.completions.create(
    model="gemini-2.5-pro",
    messages=[
        {"role": "user", "content": "Explain quantum computing"}
    ]
)

print(response.choices[0].message.content)
```

### JavaScript/TypeScript Example

```typescript
import OpenAI from 'openai';

const client = new OpenAI({
  baseURL: 'http://localhost:4000/v1',
  apiKey: 'sk-vertex-bridge-dev',
});

const completion = await client.chat.completions.create({
  model: 'gpt-4',
  messages: [{ role: 'user', content: 'Hello!' }],
});

console.log(completion.choices[0].message.content);
```

## Environment Configuration

### Required Variables

```bash
# Server Configuration
APP_SERVER__HOST=0.0.0.0
APP_SERVER__PORT=4000

# Authentication (Single Key for All Providers)
APP_AUTH__REQUIRE_AUTH=true
APP_AUTH__MASTER_KEY=sk-your-master-key-here

# Vertex AI (for gemini-* models)
APP_VERTEX__PROJECT_ID=your-gcp-project-id
APP_VERTEX__REGION=us-central1
GOOGLE_APPLICATION_CREDENTIALS=/path/to/service-account.json

# Internal Bridge Services (Implementation Details)
APP_ANTHROPIC__BRIDGE_URL=http://anthropic-bridge:4001
APP_OPENAI__HARVESTER_URL=http://harvester:3001
```

### Optional Variables

```bash
# Rate Limiting
APP_RATE_LIMIT__CAPACITY=1000
APP_RATE_LIMIT__REFILL_PER_SECOND=100

# Circuit Breaker
APP_CIRCUIT_BREAKER__FAILURE_THRESHOLD=10
APP_CIRCUIT_BREAKER__TIMEOUT_SECS=60
APP_CIRCUIT_BREAKER__SUCCESS_THRESHOLD=3

# Caching
APP_CACHE__ENABLED=false
APP_CACHE__DEFAULT_TTL_SECS=3600

# Logging
APP_LOG__LEVEL=info
APP_LOG__FORMAT=json
```

## Deployment

### Docker Compose

The main proxy and internal bridge services run together:

```bash
docker-compose up -d
```

This starts:

- **vertex-bridge** (main proxy) on port 4000 - **This is your single entry point**
- **anthropic-bridge** (internal) on port 4001 - Not exposed externally
- **harvester** (internal) on port 3001 - Not exposed externally

**Important**: Only port 4000 needs to be exposed. The bridge services (4001, 3001) are internal implementation details and should not be accessed directly by clients.

### Kubernetes

The main proxy service exposes port 4000. Internal bridge services are separate deployments with ClusterIP services (not exposed externally).

### Health Check

Verify the proxy is running:

```bash
curl http://localhost:4000/health
```

Expected response:

```json
{
  "status": "healthy",
  "version": "0.1.0",
  "providers": {
    "Vertex": {"state": "Closed"},
    "AnthropicCLI": {"state": "Closed"}
  }
}
```

## Internal Architecture Details

While clients only interact with port 4000, the proxy internally routes to:

1. **Anthropic Bridge** (`anthropic-bridge:4001`):
   - Node.js service that wraps Anthropic CLI
   - Converts OpenAI format → Anthropic CLI format
   - Requires authenticated `claude` CLI in container

2. **OpenAI Harvester** (`harvester:3001`):
   - Node.js service managing browser sessions
   - Extracts tokens from ChatGPT web interface
   - Handles Arkose challenges for GPT-4

3. **Vertex AI**:
   - Direct API calls from main proxy
   - Uses `GOOGLE_APPLICATION_CREDENTIALS` for authentication
   - No intermediate service required

**Note**: These internal services are implementation details. Clients should never connect to ports 4001 or 3001 directly.

## Migration from LiteLLM

If you were previously using LiteLLM as a router:

1. **Remove LiteLLM**: No longer needed
2. **Update Base URL**: Change from LiteLLM port (typically 4000) to FkLLMProxy port 4000
3. **Update API Key**: Use `APP_AUTH__MASTER_KEY` instead of LiteLLM's master key
4. **Model Names**: Keep the same model names (they route automatically)

The proxy provides the same unified interface that LiteLLM was providing, but with direct provider integration.
