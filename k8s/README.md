# Kubernetes Deployment

This directory contains Kubernetes manifests for deploying FkLLMProxy to a Kubernetes cluster.

## Prerequisites

- Kubernetes cluster (1.20+)
- `kubectl` configured to access your cluster
- Docker images built and pushed to a registry

## Quick Start

1. **Build and push Docker images**:

```bash
# Build images
docker build -t your-registry/fkllmproxy:latest .
docker build -t your-registry/fkllmproxy-harvester:latest ./harvester
docker build -t your-registry/fkllmproxy-anthropic-bridge:latest ./bridge

# Push to registry
docker push your-registry/fkllmproxy:latest
docker push your-registry/fkllmproxy-harvester:latest
docker push your-registry/fkllmproxy-anthropic-bridge:latest
```

2. **Update image references in `deployment.yaml`**:

Replace `fkllmproxy:latest` with your registry path.

3. **Create secrets**:

```bash
# Create master key secret
kubectl create secret generic fkllmproxy-secrets \
  --from-literal=master-key='sk-your-secret-key'

# Create GCP credentials secret (base64 encode your service account JSON)
kubectl create secret generic fkllmproxy-gcp-credentials \
  --from-file=sa.json=/path/to/service-account.json
```

4. **Update ConfigMap**:

Edit `configmap.yaml` with your GCP project ID and region.

5. **Deploy**:

```bash
kubectl apply -f k8s/
```

## Files

- `deployment.yaml` - Deployment manifests for all services
- `service.yaml` - Service definitions for internal communication
- `configmap.yaml` - Non-sensitive configuration
- `secrets.yaml.example` - Example secret structure (create actual secrets via kubectl)

## Monitoring

The deployments include liveness and readiness probes. Metrics are available at `/metrics/prometheus` endpoint.

## Scaling

To scale the proxy:

```bash
kubectl scale deployment fkllmproxy --replicas=3
```

## Troubleshooting

- Check pod logs: `kubectl logs -l app=fkllmproxy,component=proxy`
- Check service endpoints: `kubectl get endpoints fkllmproxy`
- Describe pod: `kubectl describe pod <pod-name>`
