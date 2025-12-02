#!/bin/bash

# Test script for FkLLMProxy with streaming
# Tests streaming chat completions

set -e
set -o pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
BASE_URL="${APP_SERVER__HOST:-127.0.0.1}:${APP_SERVER__PORT:-4000}"
AUTH_REQUIRED="${APP_AUTH__REQUIRE_AUTH:-false}"
MASTER_KEY="${APP_AUTH__MASTER_KEY:-sk-vertex-bridge-changeme}"
TEST_MODEL="${TEST_MODEL:-gemini-2.5-flash}"

# Build auth header
if [ "$AUTH_REQUIRED" = "true" ]; then
    AUTH_HEADER="Authorization: Bearer $MASTER_KEY"
else
    AUTH_HEADER=""
fi

echo -e "${BLUE}=== FkLLMProxy Streaming Test ===${NC}\n"
echo -e "Model: ${BLUE}$TEST_MODEL${NC}"
echo -e "Endpoint: ${BLUE}http://${BASE_URL}/v1/chat/completions${NC}\n"

# Prepare streaming request
REQUEST_BODY=$(cat <<EOF
{
  "model": "$TEST_MODEL",
  "messages": [
    {"role": "user", "content": "Count from 1 to 5, one number per line."}
  ],
  "stream": true,
  "max_tokens": 50
}
EOF
)

echo -e "${YELLOW}Streaming response:${NC}\n"

# Build curl command for streaming
CURL_CMD="curl -s -N -X POST http://${BASE_URL}/v1/chat/completions"
CURL_CMD="${CURL_CMD} -H 'Content-Type: application/json'"

if [ -n "$AUTH_HEADER" ]; then
    CURL_CMD="${CURL_CMD} -H '${AUTH_HEADER}'"
fi

CURL_CMD="${CURL_CMD} -d '${REQUEST_BODY}'"

# Execute streaming request
eval $CURL_CMD | while IFS= read -r line; do
    if [[ $line == data:* ]]; then
        # Extract JSON from SSE format
        json_data="${line#data: }"
        if [ "$json_data" != "[DONE]" ] && [ -n "$json_data" ]; then
            if command -v jq &> /dev/null; then
                content=$(echo "$json_data" | jq -r '.choices[0].delta.content // empty' 2>/dev/null)
                if [ -n "$content" ]; then
                    echo -n "$content"
                fi
            else
                echo "$json_data"
            fi
        fi
    fi
done

echo -e "\n\n${GREEN}=== Streaming Test Complete ===${NC}"

