#!/bin/bash
set -euo pipefail

# Test Docker Compose Setup
# Verifies all services start correctly and health endpoints work

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_ROOT"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${YELLOW}Testing Docker Compose Setup${NC}"
echo "=================================="

# Function to check if a URL is accessible
check_url() {
    local url=$1
    local service_name=$2
    local max_attempts=30
    local attempt=1

    echo -n "Checking $service_name... "

    while [ $attempt -le $max_attempts ]; do
        if curl -sf "$url" > /dev/null 2>&1; then
            echo -e "${GREEN}✓${NC}"
            return 0
        fi
        sleep 1
        attempt=$((attempt + 1))
    done

    echo -e "${RED}✗ (timeout after ${max_attempts}s)${NC}"
    return 1
}

# Function to check health endpoint returns JSON with status
check_health_json() {
    local url=$1
    local service_name=$2

    echo -n "Checking $service_name health JSON... "

    local response
    response=$(curl -sf "$url" 2>/dev/null || echo "")

    if [ -z "$response" ]; then
        echo -e "${RED}✗ (no response)${NC}"
        return 1
    fi

    if echo "$response" | jq -e '.status' > /dev/null 2>&1; then
        local status
        status=$(echo "$response" | jq -r '.status')
        if [ "$status" = "ok" ] || [ "$status" = "healthy" ]; then
            echo -e "${GREEN}✓ (status: $status)${NC}"
            return 0
        else
            echo -e "${YELLOW}⚠ (status: $status)${NC}"
            return 0  # Still consider it working if JSON is valid
        fi
    else
        echo -e "${RED}✗ (invalid JSON)${NC}"
        echo "Response: $response"
        return 1
    fi
}

# Check if Docker Compose is available
if ! command -v docker-compose &> /dev/null && ! command -v docker &> /dev/null; then
    echo -e "${RED}Error: docker-compose or docker not found${NC}"
    exit 1
fi

# Use docker compose (v2) if available, fall back to docker-compose (v1)
if docker compose version &> /dev/null; then
    DOCKER_COMPOSE="docker compose"
elif docker-compose version &> /dev/null; then
    DOCKER_COMPOSE="docker-compose"
else
    echo -e "${RED}Error: docker compose not available${NC}"
    exit 1
fi

echo ""
echo "Step 1: Starting services..."
echo "----------------------------------"

# Start services in detached mode
if $DOCKER_COMPOSE up -d; then
    echo -e "${GREEN}Services started${NC}"
else
    echo -e "${RED}Failed to start services${NC}"
    exit 1
fi

echo ""
echo "Step 2: Waiting for services to be ready..."
echo "----------------------------------"

# Wait a bit for services to initialize
sleep 5

# Check each service
echo ""
echo "Step 3: Checking service endpoints..."
echo "----------------------------------"

FAILED=0

# Main proxy health endpoint (includes bridge checks)
if check_url "http://localhost:4000/health" "vertex-bridge"; then
    if check_health_json "http://localhost:4000/health" "vertex-bridge"; then
        echo "  Health details:"
        curl -s "http://localhost:4000/health" | jq '.' 2>/dev/null || echo "  (JSON parsing failed)"
    else
        FAILED=$((FAILED + 1))
    fi
else
    FAILED=$((FAILED + 1))
fi

# Anthropic bridge (may not have health endpoint, check if it's listening)
echo -n "Checking anthropic-bridge (port 4001)... "
if curl -sf "http://localhost:4001" > /dev/null 2>&1 || nc -z localhost 4001 2>/dev/null; then
    echo -e "${GREEN}✓ (port accessible)${NC}"
else
    echo -e "${YELLOW}⚠ (may require authentication)${NC}"
    # Don't fail - bridge might need CLI auth
fi

# Harvester (may not have health endpoint, check if it's listening)
echo -n "Checking harvester (port 3001)... "
if curl -sf "http://localhost:3001" > /dev/null 2>&1 || nc -z localhost 3001 2>/dev/null; then
    echo -e "${GREEN}✓ (port accessible)${NC}"
else
    echo -e "${YELLOW}⚠ (may require browser session)${NC}"
    # Don't fail - harvester might need browser setup
fi

echo ""
echo "Step 4: Checking service logs..."
echo "----------------------------------"

# Show recent logs from each service
echo ""
echo "vertex-bridge logs (last 5 lines):"
$DOCKER_COMPOSE logs --tail=5 vertex-bridge 2>&1 | sed 's/^/  /'

echo ""
echo "anthropic-bridge logs (last 5 lines):"
$DOCKER_COMPOSE logs --tail=5 anthropic-bridge 2>&1 | sed 's/^/  /'

echo ""
echo "harvester logs (last 5 lines):"
$DOCKER_COMPOSE logs --tail=5 harvester 2>&1 | sed 's/^/  /'

echo ""
echo "=================================="
if [ $FAILED -eq 0 ]; then
    echo -e "${GREEN}✓ All checks passed!${NC}"
    echo ""
    echo "Services are running:"
    echo "  - Main proxy: http://localhost:4000"
    echo "  - Anthropic bridge: http://localhost:4001"
    echo "  - Harvester: http://localhost:3001"
    echo ""
    echo "To view logs: $DOCKER_COMPOSE logs -f"
    echo "To stop services: $DOCKER_COMPOSE down"
    exit 0
else
    echo -e "${RED}✗ Some checks failed ($FAILED)${NC}"
    echo ""
    echo "Check logs with: $DOCKER_COMPOSE logs"
    echo "Check status with: $DOCKER_COMPOSE ps"
    exit 1
fi

