# ADR 004: Split-Process Architecture (Harvester & Bridge)

**Status**: Accepted
**Date**: 2025-11-30
**Deciders**: Architecture Team

## Context

We needed to support providers that do not have a standard public API compatible with our server-side environment:

1. **OpenAI (via Web Interface)**: Requires browser automation to harvest tokens and manage sessions, which is complex to implement robustly in pure Rust.
2. **Anthropic (via CLI)**: Requires interacting with the official `claude` CLI tool which handles its own authentication and state.

## Decision

We adopted a **Split-Process / Sidecar Architecture**:

1. **Core Proxy (Rust)**: Handles the HTTP/SSE layer, authentication, rate limiting, and routing.
2. **Harvester (Node.js/Puppeteer)**: A sidecar service that runs a headless browser to interface with ChatGPT Web.
3. **Bridge (Node.js)**: A sidecar service that wraps the `claude` CLI and exposes it via HTTP.

The Rust proxy delegates requests for these specific providers to the respective sidecar services via HTTP.

## Consequences

### Positive

- **Tooling Suitability**: Node.js is superior for browser automation (Puppeteer/Playwright) and CLI wrapping.
- **Isolation**: Crashes in the browser automation layer do not take down the main proxy.
- **Reuse**: We can reuse existing JS-based automation logic.

### Negative

- **Deployment Complexity**: Users must run multiple containers/processes (Docker Compose becomes essential).
- **Latency**: Extra HTTP hop between proxy and sidecar.
- **Resource Usage**: Running a headless browser (even headless) consumes significant memory.

## Implementation

- `src/services/providers/anthropic.rs`: Rust client for the Anthropic bridge.
- `src/services/providers/openai.rs`: Rust client for the Harvester.
- `harvester/`: TypeScript source for the OpenAI session manager.
- `bridge/`: TypeScript source for the Anthropic CLI wrapper.
