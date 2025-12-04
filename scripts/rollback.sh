#!/bin/bash
# scripts/rollback.sh
# Emergency rollback to previous deployment
set -euo pipefail

NAMESPACE="${NAMESPACE:-default}"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
NC='\033[0m'

error() { echo -e "${RED}❌ $1${NC}"; }

echo -e "${YELLOW}⚠️  EMERGENCY ROLLBACK${NC}"
echo ""

# Check cluster connectivity
if ! kubectl cluster-info &> /dev/null; then
    error "Cannot connect to Kubernetes cluster"
    exit 1
fi

# Show current status
echo "Current deployment status:"
if ! kubectl get deployments -n "$NAMESPACE" -l app=fkllmproxy -o wide 2>/dev/null; then
    error "Failed to get deployment status. Cluster unreachable or namespace invalid."
    exit 1
fi
echo ""

# Show revision history
echo "Revision history (vertex-bridge):"
kubectl rollout history deployment/fkllmproxy -n "$NAMESPACE" 2>/dev/null | tail -5 || echo "  (no history available)"
echo ""

# Verify deployments exist before rollback
deployments_exist=0
kubectl get deployment/fkllmproxy -n "$NAMESPACE" &>/dev/null && deployments_exist=$((deployments_exist + 1))
kubectl get deployment/fkllmproxy-harvester -n "$NAMESPACE" &>/dev/null && deployments_exist=$((deployments_exist + 1))
kubectl get deployment/fkllmproxy-anthropic-bridge -n "$NAMESPACE" &>/dev/null && deployments_exist=$((deployments_exist + 1))

if [ "$deployments_exist" -eq 0 ]; then
    error "No fkllmproxy deployments found in namespace '$NAMESPACE'"
    exit 1
fi

# Confirm - skip in CI/non-interactive mode
if [ -t 0 ] && [ -z "${CI:-}" ] && [ -z "${FORCE_ROLLBACK:-}" ]; then
    read -p "Rollback all fkllmproxy deployments? [y/N] " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        echo "Cancelled"
        exit 0
    fi
else
    echo "Non-interactive mode: auto-confirming rollback"
fi

# Execute rollback with error tracking
echo ""
echo "Rolling back..."

rollback_failed=0

if kubectl get deployment/fkllmproxy -n "$NAMESPACE" &>/dev/null; then
    kubectl rollout undo deployment/fkllmproxy -n "$NAMESPACE" || {
        error "Failed to rollback fkllmproxy"
        rollback_failed=1
    }
fi

if kubectl get deployment/fkllmproxy-harvester -n "$NAMESPACE" &>/dev/null; then
    kubectl rollout undo deployment/fkllmproxy-harvester -n "$NAMESPACE" || {
        error "Failed to rollback fkllmproxy-harvester"
        rollback_failed=1
    }
fi

if kubectl get deployment/fkllmproxy-anthropic-bridge -n "$NAMESPACE" &>/dev/null; then
    kubectl rollout undo deployment/fkllmproxy-anthropic-bridge -n "$NAMESPACE" || {
        error "Failed to rollback fkllmproxy-anthropic-bridge"
        rollback_failed=1
    }
fi

# Wait for rollback and verify health
echo ""
echo "Waiting for rollback..."

if kubectl get deployment/fkllmproxy -n "$NAMESPACE" &>/dev/null; then
    if ! kubectl rollout status deployment/fkllmproxy -n "$NAMESPACE" --timeout=120s; then
        error "Rollback status check failed for fkllmproxy"
        rollback_failed=1
    fi
fi

if kubectl get deployment/fkllmproxy-harvester -n "$NAMESPACE" &>/dev/null; then
    if ! kubectl rollout status deployment/fkllmproxy-harvester -n "$NAMESPACE" --timeout=120s; then
        error "Rollback status check failed for fkllmproxy-harvester"
        rollback_failed=1
    fi
fi

if kubectl get deployment/fkllmproxy-anthropic-bridge -n "$NAMESPACE" &>/dev/null; then
    if ! kubectl rollout status deployment/fkllmproxy-anthropic-bridge -n "$NAMESPACE" --timeout=120s; then
        error "Rollback status check failed for fkllmproxy-anthropic-bridge"
        rollback_failed=1
    fi
fi

echo ""
if [ "$rollback_failed" -eq 1 ]; then
    error "Rollback completed with errors - manual intervention may be required"
    kubectl get pods -n "$NAMESPACE" -l app=fkllmproxy
    exit 1
fi

echo -e "${GREEN}✅ Rollback complete${NC}"
echo ""
kubectl get pods -n "$NAMESPACE" -l app=fkllmproxy

