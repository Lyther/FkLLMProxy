# [02-BUILD] Deployment (The Shipment)

> **Manifesto**: Deployment should be one command. Complexity is a bug.

## 🚨 Critical Constraints

### 1. Docker Compose Over Manual Docker Run

**REQUIRED**: Provide `docker-compose.yml` for one-click startup.

- **BANNED**: Complex `docker run` commands with 10+ flags
- **REQUIRED**: `docker compose up -d` should work after copying `.env.example` to `.env`
- **RULE**: All environment variables should be documented in `.env.example` with comments

### 2. Service Account File Handling

**NEVER** commit service account JSON files.

- **BANNED**: `git add service-account.json`
- **REQUIRED**: Add pattern to `.gitignore`: `*-*.json` or `service-account*.json`
- **REQUIRED**: Mount service account as volume in docker-compose: `./service-account.json:/etc/gcp/sa.json:ro`
- **RULE**: Use descriptive filenames like `{project-id}-{key-id}.json` for clarity

### 3. Vertex AI API Enablement

**REQUIRED**: Document that Vertex AI API must be enabled in GCP Console.

- **ERROR**: `403 PERMISSION_DENIED: SERVICE_DISABLED`
- **FIX**: Enable at `https://console.developers.google.com/apis/api/aiplatform.googleapis.com/overview?project={project-id}`
- **RULE**: Add this to README prerequisites, not buried in troubleshooting

### 4. Health Check Endpoint

**REQUIRED**: Health endpoint should work without auth when auth is enabled.

- **CURRENT**: `/health` requires auth → returns `401`
- **BETTER**: `/health` should bypass auth middleware (or use different route)
- **RULE**: Health checks are for monitoring, not user requests

## 📋 Deployment Checklist

- [ ] `docker-compose.yml` exists and works with `.env.example`
- [ ] Service account JSON is in `.gitignore`
- [ ] README documents Vertex AI API enablement step
- [ ] Health endpoint accessible without auth
- [ ] All required env vars documented in `.env.example`

