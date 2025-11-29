# Vertex Bridge

A high-performance Rust proxy that connects OpenAI-compatible clients (like Cursor, VSCode) to Google Vertex AI (Gemini).

## 🚀 Quick Start

### 1. Prerequisites

- **Option A (Personal)**: A Google AI Studio API Key (Get one [here](https://aistudio.google.com/)).
- **Option B (Enterprise)**: A Google Cloud Service Account with `Vertex AI User` role.

### 2. Configuration

#### Option A: Using API Key (Recommended for Individuals)

Set the environment variable:

```bash
export GOOGLE_API_KEY="AIzaSy..."
```

#### Option B: Using Service Account (Recommended for Production)

1. Place your service account key in the project root (e.g., `service-account.json`).
2. Set the environment variable:

   ```bash
   export GOOGLE_APPLICATION_CREDENTIALS=$(pwd)/service-account.json
   ```

### 3. Run

```bash
cargo run
```

The server will start at `http://127.0.0.1:4000`.

### 4. Connect Cursor

1. Go to **Cursor Settings** -> **Models**.
2. Add a custom model: `gemini-flash-latest` (or `gemini-pro-latest`).
3. Set **OpenAI Base URL** to `http://localhost:4000/v1`.
4. Set **API Key** to `sk-vertex-bridge-dev` (or whatever you set in `.env`).

## 🧪 Testing

```bash
curl -X POST http://localhost:4000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer sk-vertex-bridge-dev" \
  -d '{
    "model": "gemini-flash-latest",
    "messages": [
      {"role": "user", "content": "Hello, who are you?"}
    ]
  }'
```

## 🏗️ Architecture

- **Rust/Axum**: High-performance async web server.
- **Dual Auth Mode**: Supports both Google AI Studio (API Key) and Vertex AI (Service Account).
- **Transformer**: Maps OpenAI JSON to Vertex/Gemini JSON on the fly.
