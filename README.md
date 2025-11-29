# Vertex Bridge

A high-performance Rust proxy that connects OpenAI-compatible clients (like Cursor, VSCode) to Google Gemini (via **Google AI Studio** or **Vertex AI**).

## 🚀 Quick Start

### 1. Get a Google API Key

1. Open **Google AI Studio**: <https://aistudio.google.com/app/apikey>
2. Click **"Create API key"** and choose a Cloud Project.
3. Copy the key (`AIzaSy...`).

### 2. Configure

Copy `.env.example` to `.env` and fill in your values:

```bash
cp .env.example .env
```

```env
# Required
GOOGLE_API_KEY=AIzaSy...

# Optional: Auth for the proxy itself
APP_AUTH__REQUIRE_AUTH=true
APP_AUTH__MASTER_KEY=sk-your-secret-key
```

### 3. Run

#### Docker (Recommended)

```bash
docker build -t vertex-bridge .
docker run -d --name vertex-bridge -p 4000:4000 --env-file .env vertex-bridge
```

#### Local

```bash
cargo run
```

Server starts at `http://0.0.0.0:4000`.

### 4. Connect Cursor

1. **Cursor Settings → Models**
2. Add model: `gemini-2.0-flash` or `gemini-pro`
3. **OpenAI Base URL**: `http://localhost:4000/v1`
4. **API Key**: your `APP_AUTH__MASTER_KEY` value (e.g. `sk-vertex-bridge-kfccrazythursdayvme50yuan`)

## 🧪 Test

```bash
curl -X POST http://localhost:4000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer sk-vertex-bridge-kfccrazythursdayvme50yuan" \
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

- **Rust / Axum**: High-performance async web server
- **Dual Auth**: Google AI Studio (API Key) or Vertex AI (Service Account)
- **Transformer**: OpenAI JSON ↔ Gemini JSON on the fly
