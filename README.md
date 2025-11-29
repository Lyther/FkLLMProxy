# Vertex Bridge

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
2. Download the JSON key and place it in the project root, e.g. `service-account.json`.
3. Set:

   ```bash
   export GOOGLE_APPLICATION_CREDENTIALS="$(pwd)/service-account.json"
   ```

In this mode the bridge talks to `aiplatform.googleapis.com` (Vertex AI).

### 3. Run

```bash
cargo run
```

Server starts at `http://0.0.0.0:4000`.

### 4. Connect Cursor

1. Go to **Cursor Settings → Models**.
2. Add a custom model, e.g. `gemini-flash-latest` (or `gemini-pro-latest`).
3. Set **OpenAI Base URL** to: `http://localhost:4000/v1`.
4. Set **API Key** (the *client*-side key) to something like
   `sk-vertex-bridge-dev` (or whatever you configure in `.env` / config).

> This “API Key” is **just for your local bridge** and unrelated to the Google API key.
> The bridge itself uses `GOOGLE_API_KEY` or the service account credentials to talk to Google.

## 🧪 Testing

```bash
curl -X POST http://localhost:4000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer sk-vertex-bridge-dev" \
  -d '{
    "model": "gemini-2.0-flash",
    "messages": [{"role": "user", "content": "Hello!"}]
  }'
```

## 📝 Environment Variables

| Variable | Required | Description |
|----------|----------|-------------|
| `GOOGLE_API_KEY` | Yes* | Google AI Studio API key |
| `GOOGLE_APPLICATION_CREDENTIALS` | Yes* | Path to service account JSON (alternative to API key) |
| `APP_SERVER__HOST` | No | Bind address (default: `127.0.0.1`) |
| `APP_SERVER__PORT` | No | Port (default: `4000`) |
| `APP_AUTH__REQUIRE_AUTH` | No | Enable auth (default: `false`) |
| `APP_AUTH__MASTER_KEY` | No | API key for clients to use |
| `APP_LOG__LEVEL` | No | Log level (default: `info`) |

\* One of `GOOGLE_API_KEY` or `GOOGLE_APPLICATION_CREDENTIALS` is required.

## 🏗️ Architecture

- **Rust / Axum**: High-performance async web server.
- **Dual Auth Mode**: Supports both **Google AI Studio (API Key)** and **Vertex AI (Service Account)**.
- **Transformer**: Maps OpenAI-compatible JSON to Gemini / Vertex JSON on the fly.
