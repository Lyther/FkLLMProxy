# ICP Harvester

Browser automation service for OpenAI session management and token extraction.

## Setup

```bash
npm install
npm run dev
```

## Configuration

- `PORT`: Server port (default: 3001)
- `HOST`: Server host (default: 127.0.0.1)

## Endpoints

- `GET /health` - Browser and session health check
- `GET /tokens` - Get access token and optional Arkose token
- `POST /refresh` - Force token refresh (with optional `force_arkose: true`)

## Session Management

- Cookies are persisted to `cookies.json` for session recovery
- Keep-alive runs every 5 minutes to maintain session
- Access tokens are extracted from `/api/auth/session` responses

## Arkose Token

- Required for GPT-4 model requests
- Cached for 2 minutes
- Generated via browser automation

