# Design Proposal: Iron-Clad Proxy (ICP)

**High-Fidelity OpenAI Session Bridge via TLS Impersonation**

**Author:** Catherine Li
**Date:** November 2025
**Status:** ✅ **IMPLEMENTED** - See `src/openai/` and `harvester/` for actual implementation.

## 1. Executive Summary

This document outlines the architecture for **Iron-Clad Proxy (ICP)**, a middleware solution designed to bridge standard OpenAI API clients (e.g., Cursor, VS Code) with the undocumented OpenAI ChatGPT Web Interface.

Unlike fragile Python-based proxies, ICP leverages **Rust** for high-performance request handling and **TypeScript (Playwright)** for accurate browser environment simulation to handle dynamic JavaScript challenges (Arkose/FunCaptcha). **Note**: TLS fingerprint impersonation (`reqwest-impersonate`) is planned but not yet implemented - currently using standard `reqwest` which may be blocked by WAF.

## 2. System Architecture

The system follows a **Split-Process Architecture** to enforce separation of concerns:

1. **The Enforcer (Rust):** High-performance HTTP proxy and stream transformation. (TLS fingerprint spoofing planned but not yet implemented)
2. **The Harvester (TypeScript):** Headless browser automation for session lifecycle management and challenge solving.

### 2.1 High-Level Data Flow

```mermaid
[Cursor IDE] --(1) Standard API Request--> [ICP Enforcer (Rust)]
                                                |
                                     (2) Request Auth/Arkose Token
                                                v
                                     [ICP Harvester (Node/TS)] --(3) CDP/Browser--> [ChatGPT Web]
                                                |
                                     (4) Return Fresh Tokens
                                                v
[ICP Enforcer] --(5) Spoofed TLS Request (JA3/JA4)--> [OpenAI Backend]
      ^                                                 |
      |_________________(6) SSE Stream__________________|
```

## 3. Component Specification

### 3.1 The Enforcer (Core Logic)

* **Language:** Rust (2024 Edition)
* **Responsibility:** Handling high-throughput traffic, enforcing protocol compliance. (WAF evasion via TLS fingerprinting is planned but not yet implemented)
* **Key Libraries:**
  * `axum`: Asynchronous HTTP server framework for the OpenAI-compatible endpoint (`/v1/chat/completions`).
  * `reqwest`: Standard HTTP client (TLS fingerprint impersonation via `reqwest-impersonate` is planned but not yet implemented - see `src/openai/backend.rs` TODO comment)
  * `tokio`: Async runtime.
  * `serde`: High-performance JSON serialization/deserialization for payload transformation.
* **Logic:**
    1. Intercepts `POST /v1/chat/completions`.
    2. Queries **The Harvester** (via internal localhost HTTP) for a valid `access_token` and `arkose_token`.
    3. Transforms the OpenAI JSON payload into the internal `backend-api/conversation` format (mapping `messages` to `node_id` structures).
    4. Executes the upstream request with standard headers (`User-Agent`, `Accept-Language`, `Referer`). TLS fingerprint impersonation is not yet implemented.
    5. Parses the raw Server-Sent Events (SSE) from the backend, filters out internal metadata (e.g., moderation flags), and re-streams standard OpenAI chunks to the client.

### 3.2 The Harvester (Session Manager)

* **Language:** TypeScript (Node.js 22+)
* **Responsibility:** Maintaining session validity and solving "Proof of Work" (PoW) challenges that require a full DOM execution environment.
* **Key Libraries:**
  * `playwright`: For robust headless browser orchestration (Chromium engine).
  * `fastify` or `express`: Lightweight internal API to serve tokens to the Rust Enforcer.
* **Logic:**
    1. **Initialization:** Launches a Chromium instance. Performs initial login (manual or cookie injection).
    2. **Keep-Alive:** Periodically navigates or interacts with the page to prevent session timeout.
    3. **Token Extraction:** Intercepts `fetch` requests to `https://chatgpt.com/api/auth/session` to extract the `accessToken`.
    4. **Arkose Solver:** Injects scripts to trigger the `window.arkose` callback, obtaining a fresh `arkose_token` required for GPT-4 model requests. This token is cached for short durations (<2 minutes).

## 4. Implementation Strategy (The 10/30/60 Rule)

### 4.1 The 10% (Manual - You write this)

* **The TLS Handshake Config:** Tuning `reqwest-impersonate` settings in Rust to exactly match the Playwright browser version.
* **The Stream Transformer:** The Rust state machine that converts the proprietary backend SSE format into the OpenAI standard format. This is complex and prone to breaking changes; AI often hallucinates the struct fields here.
* **Arkose Trigger:** The specific JavaScript injection in Playwright to force the generation of a funcaptcha token without a user click.

### 4.2 The 30% (Tab Complete)

* **Rust Boilerplate:** `Axum` router setup, middleware logging, error handling types (`impl IntoResponse`).
* **Type Definitions:** Structs mirroring the OpenAI API spec (Request/Response objects).
* **IPC Client:** The internal HTTP client code in Rust to talk to the Node.js service.

### 4.3 The 60% (AI Generated)

* **Unit Tests:** Generating test vectors for JSON parsing.
* **Playwright Boilerplate:** Browser launch configs, basic navigation logic, retry loops.
* **Documentation:** Swagger/OpenAPI specs and basic README files.

## 5. Risk & Mitigation

* **WAF Updates:** Cloudflare frequently changes TLS fingerprint blocking rules.
  * *Mitigation:* The Rust binary must allow runtime configuration of the impersonation target (e.g., switching from `Chrome120` to `Edge119` via config).
* **Payload Schema Change:** OpenAI changes the internal JSON structure.
  * *Mitigation:* Implement "panic-free" parsing in Rust. If the internal schema changes, fallback to a raw dump mode to aid debugging.
