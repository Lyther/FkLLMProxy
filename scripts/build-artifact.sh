#!/bin/bash
# scripts/build-artifact.sh
# Build and push multi-arch container images for FkLLMProxy
set -euo pipefail

#------------------------------------------------------------------------------
# CONFIG
#------------------------------------------------------------------------------
REGISTRY="${REGISTRY:-ghcr.io}"
REPO="${REPO:-lyther/fkllmproxy}"
PLATFORMS="${PLATFORMS:-linux/amd64,linux/arm64}"
PUSH="${PUSH:-false}"

#------------------------------------------------------------------------------
# VARS (computed)
#------------------------------------------------------------------------------
COMMIT_SHA=$(git rev-parse --short HEAD)
COMMIT_SHA_FULL=$(git rev-parse HEAD)
BUILD_DATE=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
REPO_URL="https://github.com/Lyther/FkLLMProxy"

# Image names
IMAGE_VERTEX="${REGISTRY}/${REPO}/vertex-bridge"
IMAGE_HARVESTER="${REGISTRY}/${REPO}/harvester"
IMAGE_BRIDGE="${REGISTRY}/${REPO}/anthropic-bridge"

#------------------------------------------------------------------------------
# PREFLIGHT CHECKS
#------------------------------------------------------------------------------
echo "🔍 Preflight checks..."

# Check for dirty git status
if [ -n "$(git status --porcelain)" ]; then
    echo "❌ ABORT: Git working directory is dirty."
    echo "   Commit or stash changes before building artifacts."
    git status --short
    exit 1
fi

echo "✅ Git status clean (commit: ${COMMIT_SHA})"

# Check Docker
if ! command -v docker &> /dev/null; then
    echo "❌ Docker not found. Install Docker first."
    exit 1
fi

#------------------------------------------------------------------------------
# TAGGING STRATEGY
#------------------------------------------------------------------------------
# Primary: sha-<short>
# Optional: semantic version from arg
VERSION_TAG="${1:-}"
TAGS=("sha-${COMMIT_SHA}")

if [ -n "${VERSION_TAG}" ]; then
    TAGS+=("${VERSION_TAG}")
    echo "📌 Semantic version: ${VERSION_TAG}"
fi

# Build tag arguments
TAG_ARGS=""
for tag in "${TAGS[@]}"; do
    TAG_ARGS="${TAG_ARGS} --tag \${IMAGE}:${tag}"
done

#------------------------------------------------------------------------------
# OCI LABELS
#------------------------------------------------------------------------------
LABELS=(
    "org.opencontainers.image.source=${REPO_URL}"
    "org.opencontainers.image.revision=${COMMIT_SHA_FULL}"
    "org.opencontainers.image.created=${BUILD_DATE}"
    "org.opencontainers.image.url=${REPO_URL}"
    "org.opencontainers.image.documentation=${REPO_URL}/blob/main/README.md"
    "org.opencontainers.image.vendor=FkLLMProxy"
)

LABEL_ARGS=""
for label in "${LABELS[@]}"; do
    LABEL_ARGS="${LABEL_ARGS} --label ${label}"
done

#------------------------------------------------------------------------------
# BUILDX SETUP
#------------------------------------------------------------------------------
echo "🔧 Setting up Docker Buildx..."

BUILDER_NAME="fkllmproxy-builder"
if ! docker buildx inspect "${BUILDER_NAME}" &> /dev/null; then
    docker buildx create --name "${BUILDER_NAME}" --driver docker-container --bootstrap
fi
docker buildx use "${BUILDER_NAME}"

#------------------------------------------------------------------------------
# BUILD FUNCTION
#------------------------------------------------------------------------------
build_image() {
    local name=$1
    local image=$2
    local context=$3
    local dockerfile=$4

    echo ""
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo "🏗️  Building ${name}"
    echo "    Image: ${image}"
    echo "    Tags: ${TAGS[*]}"
    echo "    Platforms: ${PLATFORMS}"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

    local tag_args=""
    for tag in "${TAGS[@]}"; do
        tag_args="${tag_args} --tag ${image}:${tag}"
    done

    local push_arg=""
    if [ "${PUSH}" = "true" ]; then
        push_arg="--push"
    else
        push_arg="--load"
        # --load doesn't support multi-arch, use single platform
        PLATFORMS="linux/amd64"
    fi

    # shellcheck disable=SC2086
    docker buildx build \
        --platform "${PLATFORMS}" \
        ${tag_args} \
        ${LABEL_ARGS} \
        --label "org.opencontainers.image.title=${name}" \
        --file "${dockerfile}" \
        ${push_arg} \
        "${context}"

    echo "✅ ${name} built successfully"
}

#------------------------------------------------------------------------------
# BUILD ALL IMAGES
#------------------------------------------------------------------------------
echo ""
echo "🚀 Starting multi-arch build..."
echo "   Registry: ${REGISTRY}"
echo "   Repo: ${REPO}"
echo "   Commit: ${COMMIT_SHA}"
echo "   Push: ${PUSH}"
echo ""

# 1. vertex-bridge (Rust)
build_image "vertex-bridge" "${IMAGE_VERTEX}" "." "Dockerfile"

# 2. harvester (Node.js + Playwright)
build_image "harvester" "${IMAGE_HARVESTER}" "harvester" "harvester/Dockerfile"

# 3. anthropic-bridge (Node.js)
build_image "anthropic-bridge" "${IMAGE_BRIDGE}" "bridge" "bridge/Dockerfile"

#------------------------------------------------------------------------------
# SUMMARY
#------------------------------------------------------------------------------
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "✅ BUILD COMPLETE"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
echo "Images:"
for tag in "${TAGS[@]}"; do
    echo "  • ${IMAGE_VERTEX}:${tag}"
    echo "  • ${IMAGE_HARVESTER}:${tag}"
    echo "  • ${IMAGE_BRIDGE}:${tag}"
done
echo ""
echo "OCI Labels:"
echo "  • source: ${REPO_URL}"
echo "  • revision: ${COMMIT_SHA_FULL}"
echo "  • created: ${BUILD_DATE}"
echo ""

if [ "${PUSH}" = "true" ]; then
    echo "📦 Images pushed to ${REGISTRY}"
else
    echo "📦 Images built locally (use PUSH=true to push)"
fi

