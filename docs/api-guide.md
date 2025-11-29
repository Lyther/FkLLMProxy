# API Guide

## Base Configuration

* **Base URL**: `http://localhost:4000/v1`
* **Auth Header**: `Authorization: Bearer <your-master-key>`

## Endpoints

### 1. Chat Completions

`POST /v1/chat/completions`

This endpoint mimics the OpenAI API. It accepts standard OpenAI chat messages and forwards them to the configured Vertex AI model.

**Example Request:**

```bash
curl -X POST http://localhost:4000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer sk-proxy-key" \
  -d '{
    "model": "gemini-flash",
    "messages": [
      {"role": "system", "content": "You are a helpful assistant."},
      {"role": "user", "content": "Explain quantum computing in one sentence."}
    ],
    "stream": true
  }'
```

### 2. Health Check

`GET /health`

Returns `200 OK` if the server is running. Use this for load balancer probes.

## Error Handling

The proxy maps upstream Vertex AI errors to standard HTTP status codes:

| Status Code | Meaning | Cause |
|-------------|---------|-------|
| `400` | Bad Request | Invalid JSON or unsupported parameters. |
| `401` | Unauthorized | Invalid Proxy API Key. |
| `429` | Too Many Requests | Rate limit exceeded (Proxy or Upstream). |
| `500` | Internal Error | Vertex AI API failure or network issue. |
| `503` | Service Unavailable | All providers are down. |

## Client Configuration

### Cursor IDE

Add this to your Cursor settings (Cmd+Shift+J):

```json
{
  "openai.apiKey": "sk-proxy-key",
  "openai.baseURL": "http://localhost:4000/v1",
  "cursor.model": "gemini-flash"
}
```

### VSCode (Continue Extension)

```json
{
  "models": [
    {
      "title": "Gemini Flash",
      "provider": "openai",
      "model": "gemini-flash",
      "apiKey": "sk-proxy-key",
      "apiBase": "http://localhost:4000/v1"
    }
  ]
}
```
