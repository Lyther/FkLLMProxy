# Documentation Map

> **Structure**: Role-Based Organization (User, Dev, Ops).

## 1. User Documentation

*For those who consume the API.*

- [`docs/user/configuration.md`](../user/configuration.md): detailed configuration guide (TOML/Env vars).
- [`README.md`](../../README.md): Quick start and feature overview.

## 2. Developer Documentation

*For those who build the proxy.*

### Architecture

- [`docs/dev/architecture/system-overview.md`](architecture/system-overview.md): High-level design and topology.
- [`docs/dev/architecture/provider-pattern.md`](architecture/provider-pattern.md): How to add new LLM providers.
- [`docs/dev/adr/`](adr/): Architecture Decision Records (History of decisions).

### API & Contracts

- [`docs/dev/api/openapi.yaml`](api/openapi.yaml): The Open API Specification (Manual).

### Testing

- [`docs/dev/testing/guide.md`](testing/guide.md): How to run and write tests.
- [`docs/dev/providers/`](providers/): Provider-specific details (Anthropic, OpenAI, Vertex).

## 3. Operations Documentation

*For those who deploy and maintain.*

- [`docs/ops/deployment.md`](../ops/deployment.md): Deployment guide (Docker, Systemd, Security).
- [`docs/ops/runbook.md`](../ops/runbook.md): Operational runbook (Troubleshooting, Metrics).

## 4. Project Management

*Planning and tracking.*

- [`docs/project/roadmap.md`](../project/roadmap.md): Future plans.
- [`docs/project/status.md`](../project/status.md): Current implementation status.
- [`docs/project/todo.md`](../project/todo.md): Task list.
