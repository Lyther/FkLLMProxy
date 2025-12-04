#!/bin/bash
# Security Audit Script for FkLLMProxy
# Runs dependency scanning, checks for secrets, and validates security practices

set -euo pipefail

echo "🔒 Running Security Audit for FkLLMProxy"
echo "========================================"
echo ""

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Check if cargo-audit is installed
if ! command -v cargo-audit &> /dev/null; then
    echo -e "${YELLOW}⚠️  cargo-audit not found. Installing...${NC}"
    cargo install cargo-audit --locked
fi

echo "1. Running cargo-audit (dependency vulnerability scan)..."
if cargo audit; then
    echo -e "${GREEN}✅ No known vulnerabilities found${NC}"
else
    echo -e "${RED}❌ Vulnerabilities detected! Please review and update dependencies.${NC}"
    exit 1
fi

echo ""
echo "2. Checking for hardcoded secrets..."
SECRET_PATTERNS=(
    "AIza[0-9A-Za-z_-]{35}"
    "sk-[0-9A-Za-z]{32,}"
    "-----BEGIN.*PRIVATE KEY-----"
    "password.*=.*['\"].*['\"]"
    "api[_-]?key.*=.*['\"].*['\"]"
)

FOUND_SECRETS=0
for pattern in "${SECRET_PATTERNS[@]}"; do
    if grep -r -i --exclude-dir=target --exclude-dir=node_modules --exclude-dir=.git --exclude="*.lock" -E "$pattern" . 2>/dev/null; then
        echo -e "${RED}⚠️  Potential secret found matching pattern: $pattern${NC}"
        FOUND_SECRETS=1
    fi
done

if [ $FOUND_SECRETS -eq 0 ]; then
    echo -e "${GREEN}✅ No hardcoded secrets detected${NC}"
else
    echo -e "${YELLOW}⚠️  Potential secrets found. Please verify these are not actual credentials.${NC}"
fi

echo ""
echo "3. Checking for unsafe code patterns..."
UNSAFE_PATTERNS=(
    "unsafe\\s*\\{"
    "\\.unwrap\\(\\)"
    "\\.expect\\(\\)"
    "#\\[allow\\(unsafe_code\\)\\]"
)

FOUND_UNSAFE=0
for pattern in "${UNSAFE_PATTERNS[@]}"; do
    if grep -r --exclude-dir=target --exclude="*.lock" -E "$pattern" src/ 2>/dev/null | grep -v "//.*test" | grep -v "#\\[cfg\\(test\\)\\]"; then
        echo -e "${YELLOW}⚠️  Unsafe pattern found: $pattern${NC}"
        FOUND_UNSAFE=1
    fi
done

if [ $FOUND_UNSAFE -eq 0 ]; then
    echo -e "${GREEN}✅ No unsafe code patterns detected${NC}"
else
    echo -e "${YELLOW}⚠️  Unsafe patterns found. Review for security implications.${NC}"
fi

echo ""
echo "4. Validating configuration security..."
if grep -r "APP_AUTH__MASTER_KEY" .env.example 2>/dev/null | grep -q "sk-"; then
    echo -e "${YELLOW}⚠️  .env.example contains example key. Ensure production uses strong keys.${NC}"
else
    echo -e "${GREEN}✅ Configuration files look secure${NC}"
fi

echo ""
echo "5. Checking file permissions..."
CRED_FILE="${GOOGLE_APPLICATION_CREDENTIALS:-$HOME/.config/fkllmproxy/service-account.json}"
if [ -f "$CRED_FILE" ]; then
    # Quote variable to handle paths with spaces
    file_perms="$(stat -c %a "$CRED_FILE" 2>/dev/null || stat -f %A "$CRED_FILE" 2>/dev/null)"
    if [ "$file_perms" != "600" ]; then
        echo -e "${YELLOW}⚠️  Credential file permissions should be 600 (got $file_perms): $CRED_FILE${NC}"
    else
        echo -e "${GREEN}✅ File permissions look correct: $CRED_FILE${NC}"
    fi
else
    echo -e "${YELLOW}⚠️  No credential file found at $CRED_FILE${NC}"
fi

echo ""
echo "========================================"
echo -e "${GREEN}✅ Security audit complete${NC}"
echo ""
echo "Recommendations:"
echo "- Run 'cargo update' regularly to get security patches"
echo "- Use secrets management in production (K8s secrets, Vault, etc.)"
echo "- Enable authentication in production (APP_AUTH__REQUIRE_AUTH=true)"
echo "- Use strong master keys (generate with: openssl rand -hex 32)"
echo "- Review and rotate credentials every 90 days"

