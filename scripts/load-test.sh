#!/bin/bash
# Load Testing Script for FkLLMProxy
# Uses hey or wrk for load testing

set -e
set -o pipefail

BASE_URL="${1:-http://localhost:4000}"
API_KEY="${2:-sk-vertex-bridge-dev}"
CONCURRENT="${3:-10}"
DURATION="${4:-30s}"

echo "🚀 Load Testing FkLLMProxy"
echo "=========================="
echo "Base URL: $BASE_URL"
echo "Concurrent: $CONCURRENT"
echo "Duration: $DURATION"
echo ""

# Check for hey or wrk
if command -v hey &> /dev/null; then
    echo "Using 'hey' for load testing..."
    hey -n 1000 -c $CONCURRENT -z $DURATION \
        -H "Authorization: Bearer $API_KEY" \
        -H "Content-Type: application/json" \
        -d '{"model":"gemini-2.5-flash","messages":[{"role":"user","content":"Hello"}],"max_tokens":10}' \
        -m POST \
        "$BASE_URL/v1/chat/completions"
elif command -v wrk &> /dev/null; then
    echo "Using 'wrk' for load testing..."
    echo "Note: wrk doesn't support POST body easily. Consider using hey instead."
    wrk -t4 -c$CONCURRENT -d$DURATION \
        -H "Authorization: Bearer $API_KEY" \
        -H "Content-Type: application/json" \
        -s scripts/load-test-lua.lua \
        "$BASE_URL/v1/chat/completions"
else
    echo "❌ Neither 'hey' nor 'wrk' found. Install one:"
    echo "  - hey: go install github.com/rakyll/hey@latest"
    echo "  - wrk: https://github.com/wg/wrk"
    exit 1
fi

echo ""
echo "✅ Load test complete"
echo ""
echo "Monitor metrics during test:"
echo "  curl $BASE_URL/metrics/prometheus | grep requests_total"

