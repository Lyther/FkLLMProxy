#!/bin/sh
# Script: verify-git-history.sh
# Verifies Git history integrity: signatures and bisectability

set -e

echo "🔍 Verifying Git History Integrity..."
echo ""

# Check 1: Commit Signatures
echo "1️⃣  Checking commit signatures..."
unsigned_count=0
total_checked=0

for commit in $(git log --format="%H" -20); do
    total_checked=$((total_checked + 1))
    if ! git verify-commit "$commit" 2>/dev/null; then
        unsigned_count=$((unsigned_count + 1))
        echo "  ❌ Unsigned: $(git log -1 --format="%h %s" "$commit")"
    fi
done

if [ $unsigned_count -eq 0 ]; then
    echo "  ✅ All checked commits are signed"
else
    echo "  ⚠️  Found $unsigned_count unsigned commit(s) out of $total_checked"
fi
echo ""

# Check 2: Bisectability (compile check)
echo "2️⃣  Checking commit bisectability (compilation)..."
oldest_commit=$(git log --format="%H" -1 --reverse)
current_commit=$(git rev-parse HEAD)

broken_commits=0
checked=0

current_branch=$(git branch --show-current)
for commit in $(git rev-list --reverse "$oldest_commit..$current_commit" | head -10); do
    checked=$((checked + 1))
    if ! git checkout -q "$commit" 2>/dev/null; then
        echo "  ⚠️  Could not checkout: $(git log -1 --format="%h %s" "$commit")"
        continue
    fi

    if ! cargo check --quiet 2>/dev/null; then
        broken_commits=$((broken_commits + 1))
        echo "  ❌ Broken: $(git log -1 --format="%h %s" "$commit")"
    fi

    git checkout -q "$current_branch" >/dev/null 2>&1 || true
done

git checkout -q "$current_branch" >/dev/null 2>&1 || true

if [ $broken_commits -eq 0 ]; then
    echo "  ✅ All checked commits compile successfully"
else
    echo "  ⚠️  Found $broken_commits broken commit(s) out of $checked"
fi
echo ""

# Check 3: Commit Message Format
echo "3️⃣  Checking commit message format..."
invalid_format=0
checked=0

for commit in $(git log --format="%H" -10); do
    checked=$((checked + 1))
    msg=$(git log -1 --format="%s" "$commit")

    # Skip merge and revert commits
    if echo "$msg" | grep -qE '^(Merge|Revert)'; then
        continue
    fi

    if ! echo "$msg" | grep -qE '^(feat|fix|docs|style|refactor|perf|test|chore|ci|build|revert)(\(.+\))?: .+'; then
        invalid_format=$((invalid_format + 1))
        echo "  ❌ Invalid format: $(git log -1 --format="%h %s" "$commit")"
    fi
done

if [ $invalid_format -eq 0 ]; then
    echo "  ✅ All commit messages follow conventional format"
else
    echo "  ⚠️  Found $invalid_format commit(s) with invalid format"
fi
echo ""

echo "✅ Verification complete"

