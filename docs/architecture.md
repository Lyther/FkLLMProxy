# Architecture Overview

FkLLMProxy is a high-performance LLM proxy service built in Rust using the Axum web framework.

## Core Components

- **Vertex Bridge (Rust)**: Main service handling OpenAI-compatible API requests
- **Provider Registry**: Routes requests to appropriate LLM providers (Anthropic, Vertex AI, OpenAI)
- **Bridge Services**: Node.js services for provider-specific integrations
- **Harvester**: Token management and Arkose challenge solving for OpenAI

## Key Features

- OpenAI-compatible API interface
- Multi-provider support with automatic routing
- Streaming response handling
- Rate limiting and authentication
- Circuit breaker pattern for reliability
- Comprehensive metrics and monitoring

## Architecture Details

See [`docs/dev/architecture/system-overview.md`](dev/architecture/system-overview.md) for detailed system architecture, data flows, and component specifications.

## Technology Stack

- **Backend**: Rust (Axum, Tokio)
- **Providers**: Anthropic Claude, Google Vertex AI, OpenAI GPT
- **Infrastructure**: Docker, Kubernetes, Terraform
- **Monitoring**: Prometheus, structured logging
- **Testing**: Comprehensive unit and integration test suites
