#!/bin/bash
# scripts/deploy.sh
# Deploy FkLLMProxy to Kubernetes with zero-downtime rolling update
set -euo pipefail

#------------------------------------------------------------------------------
# CONFIG
#------------------------------------------------------------------------------
NAMESPACE="${NAMESPACE:-default}"
REGISTRY="${REGISTRY:-ghcr.io}"
REPO="${REPO:-lyther/fkllmproxy}"
HEALTH_TIMEOUT="${HEALTH_TIMEOUT:-120}"
ROLLOUT_TIMEOUT="${ROLLOUT_TIMEOUT:-300}"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
NC='\033[0m'

#------------------------------------------------------------------------------
# USAGE
#------------------------------------------------------------------------------
usage() {
    cat <<EOF
Usage: $0 <version> [environment]

Arguments:
  version      Docker image tag (e.g., v1.2.3, sha-a1b2c3d)
  environment  Target environment: staging | production (default: staging)

Environment Variables:
  NAMESPACE       Kubernetes namespace (default: default)
  REGISTRY        Container registry (default: ghcr.io)
  REPO            Repository path (default: lyther/fkllmproxy)
  HEALTH_TIMEOUT  Health check timeout in seconds (default: 120)
  ROLLOUT_TIMEOUT Rollout timeout in seconds (default: 300)
  DRY_RUN         If set, only show what would be done

Examples:
  $0 v1.2.3              # Deploy v1.2.3 to staging
  $0 sha-a1b2c3d prod    # Deploy specific SHA to production
  DRY_RUN=1 $0 v1.2.3    # Dry run
EOF
    exit 1
}

#------------------------------------------------------------------------------
# LOGGING
#------------------------------------------------------------------------------
log() { echo -e "${BLUE}[$(date +%H:%M:%S)]${NC} $1"; }
success() { echo -e "${GREEN}✅ $1${NC}"; }
warn() { echo -e "${YELLOW}⚠️  $1${NC}"; }
error() { echo -e "${RED}❌ $1${NC}"; }
fatal() { error "$1"; exit 1; }

#------------------------------------------------------------------------------
# PREFLIGHT CHECKS
#------------------------------------------------------------------------------
preflight() {
    log "Phase 1: Pre-flight checks..."

    # Check kubectl
    if ! command -v kubectl &> /dev/null; then
        fatal "kubectl not found"
    fi

    # Check cluster access
    if ! kubectl cluster-info &> /dev/null; then
        fatal "Cannot connect to Kubernetes cluster"
    fi

    # Verify namespace exists
    if ! kubectl get namespace "$NAMESPACE" &> /dev/null; then
        fatal "Namespace '$NAMESPACE' does not exist"
    fi

    # Verify artifact exists
    log "Verifying artifact: ${IMAGE_PROXY}..."
    if command -v docker &> /dev/null; then
        if ! docker manifest inspect "$IMAGE_PROXY" &> /dev/null 2>&1; then
            warn "Cannot verify image exists (may need registry auth)"
        else
            success "Image verified: ${IMAGE_PROXY}"
        fi
    fi

    # Check current deployment status
    log "Current deployment status:"
    kubectl get deployments -n "$NAMESPACE" -l app=fkllmproxy -o wide 2>/dev/null || true

    success "Pre-flight checks passed"
}

#------------------------------------------------------------------------------
# DIFF CONFIG
#------------------------------------------------------------------------------
diff_config() {
    log "Phase 2: Config diff..."

    if [ -n "${DRY_RUN:-}" ]; then
        log "DRY RUN: Would apply the following changes:"
        kubectl diff -f k8s/ -n "$NAMESPACE" 2>/dev/null || true
        return
    fi

    # Show what will change
    local changes
    changes=$(kubectl diff -f k8s/ -n "$NAMESPACE" 2>/dev/null || true)
    if [ -n "$changes" ]; then
        warn "Config changes detected:"
        echo "$changes" | head -50
        echo ""
        # Skip confirmation in CI/non-interactive mode
        if [ -t 0 ] && [ -z "${CI:-}" ] && [ -z "${FORCE_DEPLOY:-}" ]; then
            read -p "Continue with deployment? [y/N] " -n 1 -r
            echo
            if [[ ! $REPLY =~ ^[Yy]$ ]]; then
                fatal "Deployment cancelled by user"
            fi
        else
            log "Non-interactive mode: auto-continuing"
        fi
    else
        log "No config changes detected"
    fi
}

#------------------------------------------------------------------------------
# ROLLOUT
#------------------------------------------------------------------------------
rollout() {
    log "Phase 3: Rolling update..."

    if [ -n "${DRY_RUN:-}" ]; then
        log "DRY RUN: Would update images to:"
        echo "  - vertex-bridge: ${IMAGE_PROXY}"
        echo "  - harvester: ${IMAGE_HARVESTER}"
        echo "  - anthropic-bridge: ${IMAGE_BRIDGE}"
        return
    fi

    # Record rollout for potential rollback
    local timestamp
    timestamp=$(date +%Y%m%d-%H%M%S)

    # Update images
    log "Updating vertex-bridge..."
    kubectl set image deployment/fkllmproxy \
        vertex-bridge="$IMAGE_PROXY" \
        -n "$NAMESPACE" \
        --record

    log "Updating harvester..."
    kubectl set image deployment/fkllmproxy-harvester \
        harvester="$IMAGE_HARVESTER" \
        -n "$NAMESPACE" \
        --record

    log "Updating anthropic-bridge..."
    kubectl set image deployment/fkllmproxy-anthropic-bridge \
        anthropic-bridge="$IMAGE_BRIDGE" \
        -n "$NAMESPACE" \
        --record

    # Wait for rollout
    log "Waiting for rollout (timeout: ${ROLLOUT_TIMEOUT}s)..."

    local failed=0
    kubectl rollout status deployment/fkllmproxy -n "$NAMESPACE" --timeout="${ROLLOUT_TIMEOUT}s" || failed=1
    kubectl rollout status deployment/fkllmproxy-harvester -n "$NAMESPACE" --timeout="${ROLLOUT_TIMEOUT}s" || failed=1
    kubectl rollout status deployment/fkllmproxy-anthropic-bridge -n "$NAMESPACE" --timeout="${ROLLOUT_TIMEOUT}s" || failed=1

    if [ $failed -eq 1 ]; then
        error "Rollout failed! Initiating rollback..."
        rollback
        fatal "Deployment failed and was rolled back"
    fi

    success "Rollout complete"
}

#------------------------------------------------------------------------------
# HEALTH VERIFICATION
#------------------------------------------------------------------------------
verify_health() {
    log "Phase 4: Health verification..."

    if [ -n "${DRY_RUN:-}" ]; then
        log "DRY RUN: Would verify health endpoints"
        return
    fi

    local service_ip
    service_ip=$(kubectl get svc fkllmproxy -n "$NAMESPACE" -o jsonpath='{.spec.clusterIP}' 2>/dev/null || echo "")

    if [ -z "$service_ip" ]; then
        warn "Cannot get service IP, skipping internal health check"
        return
    fi

    log "Waiting for health checks (timeout: ${HEALTH_TIMEOUT}s)..."

    local start_time
    start_time=$(date +%s)

    while true; do
        local elapsed
        elapsed=$(($(date +%s) - start_time))

        if [ $elapsed -gt "$HEALTH_TIMEOUT" ]; then
            error "Health check timeout after ${HEALTH_TIMEOUT}s"
            rollback
            fatal "Health verification failed, rolled back"
        fi

        # Check pod health via kubectl
        local ready_pods
        ready_pods=$(kubectl get pods -n "$NAMESPACE" -l app=fkllmproxy,component=proxy \
            -o jsonpath='{.items[*].status.conditions[?(@.type=="Ready")].status}' 2>/dev/null | tr ' ' '\n' | grep -c "True" || echo 0)

        local total_pods
        total_pods=$(kubectl get pods -n "$NAMESPACE" -l app=fkllmproxy,component=proxy --no-headers 2>/dev/null | wc -l | tr -d ' ')

        if [ "$ready_pods" -ge 1 ] && [ "$ready_pods" -eq "$total_pods" ]; then
            success "All pods healthy ($ready_pods/$total_pods ready)"
            break
        fi

        log "Waiting... ($ready_pods/$total_pods pods ready, ${elapsed}s elapsed)"
        sleep 5
    done

    success "Health verification passed"
}

#------------------------------------------------------------------------------
# ROLLBACK
#------------------------------------------------------------------------------
rollback() {
    warn "Initiating rollback..."

    local rollback_failed=0

    kubectl rollout undo deployment/fkllmproxy -n "$NAMESPACE" || {
        error "Failed to rollback fkllmproxy"
        rollback_failed=1
    }
    kubectl rollout undo deployment/fkllmproxy-harvester -n "$NAMESPACE" || {
        error "Failed to rollback fkllmproxy-harvester"
        rollback_failed=1
    }
    kubectl rollout undo deployment/fkllmproxy-anthropic-bridge -n "$NAMESPACE" || {
        error "Failed to rollback fkllmproxy-anthropic-bridge"
        rollback_failed=1
    }

    log "Waiting for rollback..."
    kubectl rollout status deployment/fkllmproxy -n "$NAMESPACE" --timeout=120s || {
        error "Rollback status check failed for fkllmproxy"
        rollback_failed=1
    }

    if [ $rollback_failed -eq 1 ]; then
        error "Rollback encountered errors - manual intervention may be required"
    fi
}

#------------------------------------------------------------------------------
# SUMMARY
#------------------------------------------------------------------------------
summary() {
    echo ""
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo -e "${GREEN}DEPLOYMENT COMPLETE${NC}"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo ""
    echo "Target:     ${ENV}"
    echo "Namespace:  ${NAMESPACE}"
    echo "Version:    ${VERSION}"
    echo ""
    echo "Images:"
    echo "  • ${IMAGE_PROXY}"
    echo "  • ${IMAGE_HARVESTER}"
    echo "  • ${IMAGE_BRIDGE}"
    echo ""
    echo "Status:"
    kubectl get pods -n "$NAMESPACE" -l app=fkllmproxy -o wide
    echo ""
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
}

#------------------------------------------------------------------------------
# MAIN
#------------------------------------------------------------------------------
main() {
    # Parse args
    VERSION="${1:-}"
    ENV="${2:-staging}"

    if [ -z "$VERSION" ]; then
        usage
    fi

    # Validate environment
    case "$ENV" in
        staging|stg|stage)
            ENV="staging"
            ;;
        production|prod)
            ENV="production"
            warn "DEPLOYING TO PRODUCTION"
            # Skip confirmation in CI/non-interactive mode
            if [ -t 0 ] && [ -z "${CI:-}" ] && [ -z "${FORCE_DEPLOY:-}" ]; then
                read -p "Are you sure? [y/N] " -n 1 -r
                echo
                if [[ ! $REPLY =~ ^[Yy]$ ]]; then
                    fatal "Deployment cancelled"
                fi
            else
                log "Non-interactive mode: auto-continuing production deploy"
            fi
            ;;
        *)
            fatal "Invalid environment: $ENV (use staging or production)"
            ;;
    esac

    # Build image names
    IMAGE_PROXY="${REGISTRY}/${REPO}/vertex-bridge:${VERSION}"
    IMAGE_HARVESTER="${REGISTRY}/${REPO}/harvester:${VERSION}"
    IMAGE_BRIDGE="${REGISTRY}/${REPO}/anthropic-bridge:${VERSION}"

    echo ""
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo -e "${BLUE}🚀 DEPLOYMENT: FkLLMProxy${NC}"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo ""
    echo "Environment: ${ENV}"
    echo "Version:     ${VERSION}"
    echo "Namespace:   ${NAMESPACE}"
    echo ""

    # Execute phases
    preflight
    diff_config
    rollout
    verify_health
    summary
}

main "$@"

