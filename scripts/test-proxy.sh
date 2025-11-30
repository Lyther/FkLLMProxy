#!/bin/bash

# Test script for FkLLMProxy
# Tests model support and sends a sample request

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration (can be overridden by env vars)
BASE_URL="${APP_SERVER__HOST:-127.0.0.1}:${APP_SERVER__PORT:-4000}"
AUTH_REQUIRED="${APP_AUTH__REQUIRE_AUTH:-false}"
MASTER_KEY="${APP_AUTH__MASTER_KEY:-sk-vertex-bridge-changeme}"

# Build auth header
if [ "$AUTH_REQUIRED" = "true" ]; then
    AUTH_HEADER="Authorization: Bearer $MASTER_KEY"
else
    AUTH_HEADER=""
fi

echo -e "${BLUE}=== FkLLMProxy Test Script ===${NC}\n"

# 1. Get all supported model IDs
echo -e "${YELLOW}1. Supported Model IDs:${NC}\n"

declare -a GEMINI_MODELS=(
    "gemini-3.0-pro"
    "gemini-3.0-deep-think"
    "gemini-2.5-pro"
    "gemini-2.5-flash"
    "gemini-2.5-flash-lite"
    "gemini-2.5-flash-image"
    "gemini-1.5-pro"
    "gemini-1.5-flash"
    "gemini-pro"
)

declare -a CLAUDE_MODELS=(
    "claude-3-5-sonnet"
    "claude-3-opus"
    "claude-3-sonnet"
    "claude-3-haiku"
)

declare -a OPENAI_MODELS=(
    "gpt-4"
    "gpt-4-turbo"
    "gpt-3.5-turbo"
)

echo -e "${GREEN}Gemini Models (Vertex AI):${NC}"
for model in "${GEMINI_MODELS[@]}"; do
    echo "  - $model"
done

echo -e "\n${GREEN}Claude Models (Anthropic CLI):${NC}"
for model in "${CLAUDE_MODELS[@]}"; do
    echo "  - $model"
done

echo -e "\n${GREEN}OpenAI Models (via Harvester):${NC}"
for model in "${OPENAI_MODELS[@]}"; do
    echo "  - $model"
done

# 2. Pick one model and send request
echo -e "\n${YELLOW}2. Testing Model:${NC}\n"

# Default to gemini-2.5-flash (most reliable)
TEST_MODEL="${TEST_MODEL:-gemini-2.5-flash}"
echo -e "Selected model: ${BLUE}$TEST_MODEL${NC}\n"

# Check if server is running
echo -e "${YELLOW}Checking server health...${NC}"
if curl -s -f "http://${BASE_URL}/health" > /dev/null; then
    echo -e "${GREEN}✓ Server is running${NC}\n"
else
    echo -e "${RED}✗ Server is not responding at http://${BASE_URL}/health${NC}"
    echo -e "${YELLOW}Make sure the server is running: cargo run${NC}\n"
    exit 1
fi

# Prepare request
REQUEST_BODY=$(cat <<EOF
{
  "model": "$TEST_MODEL",
  "messages": [
    {"role": "user", "content": "Say 'Hello from FkLLMProxy test!' in one sentence."}
  ],
  "max_tokens": 50,
  "temperature": 0.7
}
EOF
)

echo -e "${YELLOW}Sending test request...${NC}"
echo -e "Endpoint: ${BLUE}http://${BASE_URL}/v1/chat/completions${NC}"
echo -e "Model: ${BLUE}$TEST_MODEL${NC}\n"

# Build curl command
CURL_CMD="curl -s -X POST http://${BASE_URL}/v1/chat/completions"
CURL_CMD="${CURL_CMD} -H 'Content-Type: application/json'"

if [ -n "$AUTH_HEADER" ]; then
    CURL_CMD="${CURL_CMD} -H '${AUTH_HEADER}'"
fi

CURL_CMD="${CURL_CMD} -d '${REQUEST_BODY}'"

# Execute request
RESPONSE=$(eval $CURL_CMD)
HTTP_CODE=$(curl -s -o /dev/null -w "%{http_code}" -X POST "http://${BASE_URL}/v1/chat/completions" \
    -H "Content-Type: application/json" \
    ${AUTH_HEADER:+-H "$AUTH_HEADER"} \
    -d "${REQUEST_BODY}")

echo -e "${YELLOW}Response (HTTP $HTTP_CODE):${NC}"

if [ "$HTTP_CODE" -eq 200 ]; then
    echo -e "${GREEN}✓ Request successful${NC}\n"

    # Try to extract and display the response content
    if command -v jq &> /dev/null; then
        echo -e "${BLUE}Response content:${NC}"
        echo "$RESPONSE" | jq -r '.choices[0].message.content // .error.message // .' 2>/dev/null || echo "$RESPONSE"
    else
        echo "$RESPONSE" | head -20
        echo -e "\n${YELLOW}(Install 'jq' for better JSON formatting)${NC}"
    fi
else
    echo -e "${RED}✗ Request failed (HTTP $HTTP_CODE)${NC}\n"
    echo "$RESPONSE" | head -20
    exit 1
fi

echo -e "\n${GREEN}=== Test Complete ===${NC}"

