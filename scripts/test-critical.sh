#!/bin/bash
# @critical: Full critical test suite
# These tests must pass before deployment

set -e

echo "Running critical tests..."
cargo test --test integration -- --nocapture

echo "✓ Critical tests passed"

