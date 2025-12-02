#!/bin/bash
# scripts/rollback.sh
# Emergency rollback to previous deployment
set -euo pipefail

NAMESPACE="${NAMESPACE:-default}"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
NC='\033[0m'

echo -e "${YELLOW}⚠️  EMERGENCY ROLLBACK${NC}"
echo ""

# Show current status
echo "Current deployment status:"
kubectl get deployments -n "$NAMESPACE" -l app=fkllmproxy -o wide
echo ""

# Show revision history
echo "Revision history (vertex-bridge):"
kubectl rollout history deployment/fkllmproxy -n "$NAMESPACE" | tail -5
echo ""

# Confirm
read -p "Rollback all fkllmproxy deployments? [y/N] " -n 1 -r
echo
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo "Cancelled"
    exit 0
fi

# Execute rollback
echo ""
echo "Rolling back..."

kubectl rollout undo deployment/fkllmproxy -n "$NAMESPACE"
kubectl rollout undo deployment/fkllmproxy-harvester -n "$NAMESPACE"
kubectl rollout undo deployment/fkllmproxy-anthropic-bridge -n "$NAMESPACE"

# Wait for rollback
echo ""
echo "Waiting for rollback..."
kubectl rollout status deployment/fkllmproxy -n "$NAMESPACE" --timeout=120s
kubectl rollout status deployment/fkllmproxy-harvester -n "$NAMESPACE" --timeout=120s
kubectl rollout status deployment/fkllmproxy-anthropic-bridge -n "$NAMESPACE" --timeout=120s

echo ""
echo -e "${GREEN}✅ Rollback complete${NC}"
echo ""
kubectl get pods -n "$NAMESPACE" -l app=fkllmproxy

