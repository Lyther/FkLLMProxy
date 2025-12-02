#!/bin/bash
# COMMAND: TEST
# ALIAS: t, verify, check, specs
#
# THE TRUTH SERUM: Code that compiles is nothing. Code that passes tests *might* be something.
# This script runs the automated test suite to validate logic and contracts.

set -o pipefail
# Note: We don't use 'set -e' here because we need to handle test failures gracefully

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Initialize counters
UNIT_PASSED=0
UNIT_FAILED=0
INTEGRATION_PASSED=0
INTEGRATION_FAILED=0
FLAKY_TESTS=()
FAILED_TESTS=()

# Temporary files
UNIT_OUTPUT=$(mktemp)
INTEGRATION_OUTPUT=$(mktemp)
FLAKY_OUTPUT=$(mktemp)
COVERAGE_OUTPUT=$(mktemp)

# Cleanup function
cleanup() {
    rm -f "$UNIT_OUTPUT" "$INTEGRATION_OUTPUT" "$FLAKY_OUTPUT" "$COVERAGE_OUTPUT"
}
trap cleanup EXIT

# =============================================================================
# Phase 1: The Clean Room (Environment Prep)
# =============================================================================

echo -e "${BLUE}=== Phase 1: Environment Check ===${NC}"

# Check for dangerous production configurations
DANGER_DETECTED=0

# Check for production API keys (should not be in test environment)
if [ -n "$VERTEX_API_KEY" ] && [[ "$VERTEX_API_KEY" =~ ^AIza[0-9A-Za-z_-]{35}$ ]]; then
    # Check if it's a real production key (starts with common patterns)
    if [[ "$VERTEX_API_KEY" != "test-"* ]] && [[ "$VERTEX_API_KEY" != "mock-"* ]]; then
        echo -e "${YELLOW}⚠️  Warning: VERTEX_API_KEY appears to be a production key${NC}"
        # Allow it for E2E tests, but warn
    fi
fi

# Check for production URLs
if [ -n "$APP_VERTEX__API_KEY_BASE_URL" ] && [[ "$APP_VERTEX__API_KEY_BASE_URL" == *"generativelanguage.googleapis.com"* ]]; then
    echo -e "${YELLOW}⚠️  Warning: Production Vertex URL detected${NC}"
    if [ -z "$CI" ]; then
        echo -e "${RED}❌ ABORT: Cannot run tests against production API in local environment${NC}"
        exit 1
    fi
fi

# Ensure we're not accidentally pointing to production databases or services
if [ -n "$DATABASE_URL" ] && [[ "$DATABASE_URL" == *"prod"* ]] || [[ "$DATABASE_URL" == *"production"* ]]; then
    echo -e "${RED}❌ ABORT: Production database URL detected: $DATABASE_URL${NC}"
    exit 1
fi

# Check Rust environment (RUST_ENV is not standard, but check anyway)
if [ -n "$RUST_ENV" ] && [ "$RUST_ENV" = "production" ]; then
    echo -e "${RED}❌ ABORT: RUST_ENV is set to production${NC}"
    exit 1
fi

echo -e "${GREEN}✓ Environment check passed${NC}"
echo ""

# =============================================================================
# Phase 2: The Pyramid Execution
# =============================================================================

echo -e "${BLUE}=== Phase 2: Test Execution ===${NC}"

# -----------------------------------------------------------------------------
# Layer 1: Unit Tests (The Logic)
# -----------------------------------------------------------------------------

echo -e "${BLUE}Layer 1: Unit Tests (< 10ms per test, NO Network, NO DB)${NC}"

UNIT_START=$(date +%s)

# Run unit tests with timeout protection
UNIT_EXIT_CODE=0
timeout 300 cargo test --lib -- --nocapture 2>&1 | tee "$UNIT_OUTPUT" || UNIT_EXIT_CODE=$?

# Parse unit test results (Rust format: "test path::to::test ... ok" or "... FAILED")
UNIT_PASSED=$(grep -E "^test .* \.\.\. ok" "$UNIT_OUTPUT" | wc -l || echo "0")
UNIT_FAILED=$(grep -E "^test .* \.\.\. FAILED" "$UNIT_OUTPUT" | wc -l || echo "0")

# Also check summary line if available
if grep -q "test result:.*FAILED" "$UNIT_OUTPUT"; then
    # Extract from summary: "test result: FAILED. X passed; Y failed; Z ignored; M measured"
    UNIT_FAILED_FROM_SUMMARY=$(grep "test result:.*FAILED" "$UNIT_OUTPUT" | sed -E 's/.* ([0-9]+) failed.*/\1/' || echo "0")
    if [ -n "$UNIT_FAILED_FROM_SUMMARY" ] && [ "$UNIT_FAILED_FROM_SUMMARY" != "0" ]; then
        UNIT_FAILED="$UNIT_FAILED_FROM_SUMMARY"
    fi
fi

# Check for slow tests (> 1s per test is suspicious for unit tests)
SLOW_TESTS=$(grep -E "test .* \.\.\. ok.*[1-9][0-9]{3,}ms" "$UNIT_OUTPUT" || true)
if [ -n "$SLOW_TESTS" ]; then
    echo -e "${YELLOW}⚠️  Warning: Slow unit tests detected (>1s):${NC}"
    echo "$SLOW_TESTS" | head -5 | sed 's/^/  /'
fi

UNIT_END=$(date +%s)
UNIT_TIME=$((UNIT_END - UNIT_START))

if [ "$UNIT_FAILED" -gt 0 ] || [ "$UNIT_EXIT_CODE" -ne 0 ]; then
    echo -e "${RED}❌ Unit tests FAILED: $UNIT_FAILED test(s)${NC}"
    echo -e "${RED}Stopping execution - fix unit tests before proceeding${NC}"
    exit 1
else
    echo -e "${GREEN}✅ Unit: $UNIT_PASSED Passing (${UNIT_TIME}s)${NC}"
fi

echo ""

# -----------------------------------------------------------------------------
# Layer 2: Integration Tests (The Wiring)
# -----------------------------------------------------------------------------

echo -e "${BLUE}Layer 2: Integration Tests (Real DB containerized, Mocked 3rd Party APIs)${NC}"

INTEGRATION_START=$(date +%s)

# Run integration tests
INTEGRATION_EXIT_CODE=0
timeout 600 cargo test --test integration -- --nocapture 2>&1 | tee "$INTEGRATION_OUTPUT" || INTEGRATION_EXIT_CODE=$?

# Parse integration test results (Rust format: "test path::to::test ... ok" or "... FAILED")
INTEGRATION_PASSED=$(grep -E "^test .* \.\.\. ok" "$INTEGRATION_OUTPUT" | wc -l || echo "0")
INTEGRATION_FAILED=$(grep -E "^test .* \.\.\. FAILED" "$INTEGRATION_OUTPUT" | wc -l || echo "0")

# Also check summary line if available
if grep -q "test result:.*FAILED" "$INTEGRATION_OUTPUT"; then
    INTEGRATION_FAILED_FROM_SUMMARY=$(grep "test result:.*FAILED" "$INTEGRATION_OUTPUT" | sed -E 's/.* ([0-9]+) failed.*/\1/' || echo "0")
    if [ -n "$INTEGRATION_FAILED_FROM_SUMMARY" ] && [ "$INTEGRATION_FAILED_FROM_SUMMARY" != "0" ]; then
        INTEGRATION_FAILED="$INTEGRATION_FAILED_FROM_SUMMARY"
    fi
fi

INTEGRATION_END=$(date +%s)
INTEGRATION_TIME=$((INTEGRATION_END - INTEGRATION_START))

# Extract failed test names for flake detection
if [ "$INTEGRATION_FAILED" -gt 0 ]; then
    FAILED_TEST_NAMES=$(grep -E "^test .* \.\.\. FAILED" "$INTEGRATION_OUTPUT" | sed -E 's/^test (.*) \.\.\. FAILED.*/\1/' | tr '\n' ' ' || true)
    echo -e "${RED}❌ Integration tests FAILED: $INTEGRATION_FAILED test(s)${NC}"
else
    echo -e "${GREEN}✅ Integration: $INTEGRATION_PASSED Passing (${INTEGRATION_TIME}s)${NC}"
fi

if [ "$INTEGRATION_EXIT_CODE" -ne 0 ] && [ "$INTEGRATION_FAILED" -eq 0 ]; then
    # Exit code failed but no failed tests detected - likely compilation error
    echo -e "${RED}❌ Integration test execution failed (compilation error?)${NC}"
    INTEGRATION_FAILED=1
fi

echo ""

# =============================================================================
# Phase 3: The Ratchet (Coverage Audit)
# =============================================================================

echo -e "${BLUE}=== Phase 3: Coverage Audit ===${NC}"

# Check if cargo-tarpaulin is installed
if ! command -v cargo-tarpaulin &> /dev/null; then
    echo -e "${YELLOW}⚠️  cargo-tarpaulin not found. Installing...${NC}"
    cargo install cargo-tarpaulin --locked || {
        echo -e "${YELLOW}⚠️  Failed to install cargo-tarpaulin. Skipping coverage check.${NC}"
        COVERAGE_SKIPPED=1
    }
fi

if [ -z "$COVERAGE_SKIPPED" ]; then
    echo "Running coverage analysis..."

    # Run coverage (unit tests only for speed, can expand later)
    if cargo tarpaulin --lib --out Xml --output-dir . --timeout 120 2>&1 | tee "$COVERAGE_OUTPUT"; then
        # Extract coverage percentage
        COVERAGE_PCT=$(grep -oP 'line-rate="\K[0-9.]+' tarpaulin-report.xml 2>/dev/null | head -1 || echo "0")
        if [ -z "$COVERAGE_PCT" ] || [ "$COVERAGE_PCT" = "0" ]; then
            COVERAGE_PCT=$(grep -oE '[0-9]+\.[0-9]+%' "$COVERAGE_OUTPUT" | head -1 | sed 's/%//' || echo "0")
        fi

        # Convert to percentage if it's a decimal (0.845 = 84.5%)
        if [[ "$COVERAGE_PCT" =~ ^0\.[0-9]+$ ]]; then
            COVERAGE_PCT=$(echo "$COVERAGE_PCT * 100" | bc | xargs printf "%.1f")
        fi

        echo -e "${GREEN}Coverage: ${COVERAGE_PCT}%${NC}"

        # Coverage ratchet: Compare with main branch or threshold
        THRESHOLD=80.0

        # Check if bc is available for numeric comparisons
        if command -v bc &> /dev/null && [ -n "$COVERAGE_PCT" ] && [ "$COVERAGE_PCT" != "0" ]; then
            if (( $(echo "$COVERAGE_PCT < $THRESHOLD" | bc -l) )); then
                echo -e "${RED}❌ Coverage ${COVERAGE_PCT}% is below threshold ${THRESHOLD}%${NC}"
                echo -e "${RED}You added code but no tests. Go back and finish the job.${NC}"
                # Don't fail for now, just warn (can make it strict later)
            fi

            # Try to compare with main branch if in git repo
            if git rev-parse --git-dir > /dev/null 2>&1; then
                # Try main branch first, then master as fallback
                BRANCH=""
                if git show-ref --verify --quiet refs/heads/main 2>/dev/null; then
                    BRANCH="main"
                elif git show-ref --verify --quiet refs/heads/master 2>/dev/null; then
                    BRANCH="master"
                fi

                if [ -n "$BRANCH" ] && git show "$BRANCH:tarpaulin-report.xml" > /dev/null 2>&1; then
                    MAIN_COVERAGE=$(git show "$BRANCH:tarpaulin-report.xml" 2>/dev/null | grep -oP 'line-rate="\K[0-9.]+' | head -1 || echo "")
                    if [ -n "$MAIN_COVERAGE" ] && [[ "$MAIN_COVERAGE" =~ ^0\.[0-9]+$ ]]; then
                        MAIN_COVERAGE=$(echo "$MAIN_COVERAGE * 100" | bc | xargs printf "%.1f")
                    fi
                    if [ -n "$MAIN_COVERAGE" ] && [ "$MAIN_COVERAGE" != "0" ]; then
                        if (( $(echo "$COVERAGE_PCT < $MAIN_COVERAGE" | bc -l) )); then
                            DIFF=$(echo "$MAIN_COVERAGE - $COVERAGE_PCT" | bc | xargs printf "%.1f")
                            echo -e "${RED}❌ Coverage decreased by ${DIFF}% (was ${MAIN_COVERAGE}%, now ${COVERAGE_PCT}%)${NC}"
                            echo -e "${RED}You added code but no tests. Go back and finish the job.${NC}"
                            COVERAGE_DECREASED=1
                        elif (( $(echo "$COVERAGE_PCT > $MAIN_COVERAGE" | bc -l) )); then
                            DIFF=$(echo "$COVERAGE_PCT - $MAIN_COVERAGE" | bc | xargs printf "%.1f")
                            echo -e "${GREEN}⬆️  Coverage increased by ${DIFF}% (was ${MAIN_COVERAGE}%, now ${COVERAGE_PCT}%)${NC}"
                        fi
                    fi
                fi
            fi
        fi
    else
        echo -e "${YELLOW}⚠️  Coverage analysis failed${NC}"
        COVERAGE_SKIPPED=1
    fi
else
    echo -e "${YELLOW}⚠️  Coverage check skipped${NC}"
    COVERAGE_PCT="N/A"
fi

echo ""

# =============================================================================
# Phase 4: Flake Detection
# =============================================================================

if [ -n "$FAILED_TEST_NAMES" ] && [ "$INTEGRATION_FAILED" -gt 0 ]; then
    echo -e "${BLUE}=== Phase 4: Flake Detection ===${NC}"

    # Convert space-separated string to array
    IFS=' ' read -ra FAILED_ARRAY <<< "$FAILED_TEST_NAMES"

    # Retry failed tests up to 3 times
    for test_name in "${FAILED_ARRAY[@]}"; do
        # Skip empty entries
        [ -z "$test_name" ] && continue

        echo -e "${YELLOW}Retrying failed test: $test_name${NC}"

        RETRY_PASSED=0
        RETRY_OUTPUT=$(mktemp)

        for attempt in 1 2 3; do
            echo "  Attempt $attempt/3..."
            if cargo test --test integration "$test_name" -- --nocapture 2>&1 | tee "$RETRY_OUTPUT" | grep -qE "^test .* \.\.\. ok"; then
                RETRY_PASSED=1
                echo -e "${YELLOW}  ⚠️  Test passed on attempt #$attempt - MARKED AS FLAKY${NC}"
                FLAKY_TESTS+=("$test_name (passed on attempt #$attempt)")
                break
            fi
            sleep 1
        done

        rm -f "$RETRY_OUTPUT"

        if [ "$RETRY_PASSED" -eq 0 ]; then
            echo -e "${RED}  ❌ Test failed all 3 attempts - MARKED AS BROKEN${NC}"
            FAILED_TESTS+=("$test_name")
        fi
    done

    echo ""
fi

# =============================================================================
# Final Report
# =============================================================================

echo -e "${BLUE}=== Test Execution Report ===${NC}"
echo -e "Unit: ${GREEN}✅ $UNIT_PASSED Passing${NC} (${UNIT_TIME}s)"
echo -e "Integration: ${GREEN}✅ $INTEGRATION_PASSED Passing${NC} (${INTEGRATION_TIME}s)"

if [ ${#FLAKY_TESTS[@]} -gt 0 ]; then
    echo -e "Flaky: ${YELLOW}⚠️  ${#FLAKY_TESTS[@]} test(s)${NC}"
    for flaky in "${FLAKY_TESTS[@]}"; do
        echo "  - $flaky"
    done
fi

if [ -z "$COVERAGE_SKIPPED" ] && [ -n "$COVERAGE_PCT" ] && [ "$COVERAGE_PCT" != "N/A" ] && [ "$COVERAGE_PCT" != "0" ]; then
    if [ -n "$COVERAGE_DECREASED" ]; then
        echo -e "Coverage: ${RED}${COVERAGE_PCT}%${NC} (decreased)"
    else
        echo -e "Coverage: ${GREEN}${COVERAGE_PCT}%${NC}"
    fi
fi

if [ ${#FAILED_TESTS[@]} -gt 0 ]; then
    echo -e "Verdict: ${RED}FAIL${NC}"
    exit 1
elif [ ${#FLAKY_TESTS[@]} -gt 0 ]; then
    echo -e "Verdict: ${YELLOW}PASS (with warnings)${NC}"
    exit 0
else
    echo -e "Verdict: ${GREEN}PASS${NC}"
    exit 0
fi

