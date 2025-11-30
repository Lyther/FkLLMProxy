#!/bin/bash
# Validate OpenAPI specification
# Usage: ./scripts/validate-openapi.sh

set -e

OPENAPI_FILE="docs/dev/api/openapi.yaml"

echo "🔍 Validating OpenAPI specification..."

# Check if file exists
if [ ! -f "$OPENAPI_FILE" ]; then
    echo "❌ Error: OpenAPI file not found: $OPENAPI_FILE"
    exit 1
fi

# Check if spectral is installed
if command -v spectral &> /dev/null; then
    echo "✅ Running Spectral linting..."
    if [ -f ".spectral.yml" ]; then
        spectral lint "$OPENAPI_FILE" --ruleset .spectral.yml || {
            echo "❌ Spectral linting failed"
            exit 1
        }
    else
        spectral lint "$OPENAPI_FILE" || {
            echo "❌ Spectral linting failed"
            exit 1
        }
    fi
    echo "✅ Spectral validation passed"
else
    echo "⚠️  Spectral not installed, skipping linting"
    echo "💡 Install with: npm install -g @stoplight/spectral-cli"
fi

# Basic YAML validation
if command -v yamllint &> /dev/null; then
    echo "✅ Running YAML syntax validation..."
    yamllint "$OPENAPI_FILE" || {
        echo "❌ YAML syntax validation failed"
        exit 1
    }
    echo "✅ YAML syntax validation passed"
else
    echo "⚠️  yamllint not installed, skipping YAML validation"
fi

# Check for required fields
echo "✅ Checking required OpenAPI fields..."
if ! grep -q "openapi:" "$OPENAPI_FILE"; then
    echo "❌ Missing 'openapi' field"
    exit 1
fi

if ! grep -q "info:" "$OPENAPI_FILE"; then
    echo "❌ Missing 'info' field"
    exit 1
fi

if ! grep -q "paths:" "$OPENAPI_FILE"; then
    echo "❌ Missing 'paths' field"
    exit 1
fi

echo "✅ OpenAPI specification validation passed"

