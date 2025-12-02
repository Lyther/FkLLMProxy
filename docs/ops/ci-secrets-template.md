# CI/CD Secrets Configuration

Required secrets for GitHub Actions CI/CD pipeline.

## Required Secrets

Configure these in: **GitHub → Settings → Secrets and variables → Actions**

### Core Secrets

| Secret | Required | Description |
|--------|----------|-------------|
| `VERTEX_API_KEY` | For E2E tests | Google AI Studio API key (`AIzaSy...`) |
| `GOOGLE_CLOUD_PROJECT` | For E2E tests | GCP project ID |
| `GOOGLE_APPLICATION_CREDENTIALS` | Optional | Base64-encoded service account JSON |

### Container Registry (for releases)

| Secret | Required | Description |
|--------|----------|-------------|
| `GHCR_TOKEN` | For Docker push | GitHub Container Registry token (use `GITHUB_TOKEN` or PAT) |

### Optional Secrets

| Secret | Required | Description |
|--------|----------|-------------|
| `VERTEX_REGION` | No | GCP region (default: `us-central1`) |
| `FORCE_E2E_TESTS` | No | Force E2E tests even without credentials |

## Setup Instructions

### 1. Google AI Studio API Key

```bash
# Get from: https://aistudio.google.com/app/apikey
# Add as VERTEX_API_KEY in GitHub Secrets
```

### 2. Service Account (Alternative to API Key)

```bash
# 1. Create service account with Vertex AI User role
# 2. Download JSON key
# 3. Base64 encode it:
base64 -w 0 service-account.json > sa-base64.txt
# 4. Add content as GOOGLE_APPLICATION_CREDENTIALS secret
```

### 3. Container Registry

```bash
# Option A: Use default GITHUB_TOKEN (recommended)
# No additional setup needed - workflow uses ${{ secrets.GITHUB_TOKEN }}

# Option B: Personal Access Token
# Create PAT with packages:write scope
# Add as GHCR_TOKEN
```

## Verification

After adding secrets, trigger a CI run to verify:

```bash
git commit --allow-empty -m "chore: verify CI secrets"
git push
```

Check the Actions tab for E2E test results.

## Security Notes

- Never commit secrets to the repository
- Rotate API keys every 90 days
- Use least-privilege principle for service accounts
- Review audit logs for secret access
