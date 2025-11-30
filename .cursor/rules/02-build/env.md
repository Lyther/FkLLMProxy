# [02-BUILD] Environment Configuration (The Secrets)

> **Manifesto**: Configuration is the boundary between code and reality. Misconfigured secrets are production fires waiting to happen.

## 🚨 Critical Constraints

### 1. Docker Image Version Verification

**NEVER** assume Docker image tags exist. Always verify before using.

- **BANNED**: `FROM rust:1.94-slim-bookworm` (version doesn't exist)
- **REQUIRED**: Check available tags first: `curl -s https://hub.docker.com/v2/repositories/library/rust/tags | grep -o '"name":"[^"]*"'`
- **RULE**: Use stable, verified versions. Prefer `rust:1.91` over `rust:1.94` if unsure.

### 2. File Existence in Dockerfile

**NEVER** COPY files without verifying they exist.

- **BANNED**: `COPY vertex-bridge.toml /etc/vertex-bridge/vertex-bridge.toml` (file doesn't exist)
- **REQUIRED**: List files before COPY: `RUN ls -la vertex-bridge.toml || echo "File missing"`
- **RULE**: If a config file is optional, make it optional in Dockerfile (use `COPY --chown` with existence check or remove entirely).

### 3. Google AI Studio vs Vertex AI Authentication

**CRITICAL DISTINCTION**: These are **different services** with **different billing**.

#### Google AI Studio (Free Tier)

- **Auth**: `GOOGLE_API_KEY=AIzaSy...`
- **Endpoint**: `generativelanguage.googleapis.com`
- **Billing**: Free tier with strict rate limits (0 requests for premium models on free tier)
- **Models**: `gemini-3-pro-preview`, `gemini-2.0-flash`, etc.
- **Use Case**: Development, testing, personal projects

#### Vertex AI (Paid Credits)

- **Auth**: `GOOGLE_APPLICATION_CREDENTIALS=/path/to/service-account.json` + `APP_VERTEX__PROJECT_ID=project-id`
- **Endpoint**: `{region}-aiplatform.googleapis.com`
- **Billing**: Uses GCP project credits ($300 free trial)
- **Models**: `gemini-2.5-pro`, `gemini-2.5-flash` (NOT `gemini-3-pro-preview` on Vertex yet)
- **Use Case**: Production, enterprise, when you have credits

**RULE**:

- If user has **$300 Vertex credits**, they **MUST** use Service Account auth, NOT `GOOGLE_API_KEY`.
- If user wants **preview models** (like `gemini-3-pro-preview`), they **MUST** use `GOOGLE_API_KEY` (AI Studio).
- **NEVER** mix them. One or the other, not both.

### 4. Model Availability

**Models differ by endpoint**:

- **AI Studio**: `gemini-3-pro-preview`, `gemini-2.0-flash`, `gemini-1.5-pro`
- **Vertex AI**: `gemini-2.5-pro`, `gemini-2.5-flash`, `gemini-1.5-pro` (NO `gemini-3-pro-preview`)

**RULE**: When a model returns `404 NOT_FOUND`, check if it's available on that endpoint. Document model availability in README.

### 5. Environment Variable Parsing

The `config` crate is finicky with nested env vars (`APP_SERVER__PORT`).

- **REQUIRED**: Use explicit `.set_override_option()` for critical vars instead of relying on automatic parsing.
- **REQUIRED**: Provide defaults for all optional fields to avoid deserialization panics.
- **BANNED**: Assuming `APP_AUTH__MASTER_KEY` is optional when `APP_AUTH__REQUIRE_AUTH=true`. Validate this explicitly.

## 📋 Validation Checklist

Before deploying:

- [ ] Docker image tag exists and is verified
- [ ] All COPY'd files exist in repo
- [ ] Auth mode matches billing intent (AI Studio = free, Vertex = credits)
- [ ] Model names match endpoint availability
- [ ] All required env vars have defaults or explicit validation
- [ ] Service account JSON is in `.gitignore` (pattern: `*-*.json`)

## 🔥 Common Mistakes

1. **"I have $300 credits but hitting rate limits"** → Using `GOOGLE_API_KEY` instead of Service Account
2. **"Model not found"** → Using Vertex-only model name on AI Studio endpoint (or vice versa)
3. **"Config panic on startup"** → Missing default for optional field that's actually required in some modes
