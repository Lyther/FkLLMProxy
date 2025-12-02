#!/bin/bash
# @smoke: Fast sanity checks (< 2 minutes)
# Runs on git push - must be green to proceed

set -e
set -o pipefail

echo "Running smoke tests..."
cargo test --test integration smoke_ -- --nocapture

echo "✓ Smoke tests passed"

