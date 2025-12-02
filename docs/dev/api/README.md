# API Documentation

This directory contains the API contract and related documentation.

## Files

- `openapi.yaml` - OpenAPI 3.0.3 specification
- `api-contract.ts` - TypeScript type definitions (for reference)

## Validation

### Local Validation

Run the validation script:

```bash
./scripts/validate-openapi.sh
```

### Using Spectral (Recommended)

Install Spectral CLI:

```bash
npm install -g @stoplight/spectral-cli
```

Lint the OpenAPI spec:

```bash
spectral lint docs/dev/api/openapi.yaml
```

### Using openapi-diff

Compare API versions to detect breaking changes:

```bash
npm install -g openapi-diff

# Compare two versions
openapi-diff previous.yaml current.yaml
```

## API Endpoints

### Public Endpoints

- `GET /health` - Health check with service status
- `GET /metrics` - JSON metrics
- `GET /metrics/prometheus` - Prometheus metrics
- `POST /v1/chat/completions` - Chat completion (OpenAI-compatible)

### Authentication

All endpoints except `/health` require Bearer token authentication:

```text
Authorization: Bearer sk-your-api-key
```

## Naming Conventions

**Important**: This API uses `snake_case` for JSON properties (e.g., `max_tokens`, `finish_reason`) to maintain compatibility with the OpenAI API format. This is intentional and documented in the OpenAPI spec.

## Timestamp Formats

The `created` field uses Unix epoch integers (not ISO 8601 strings) to match OpenAI's format. This is intentional for compatibility.

## Breaking Changes

API versioning is handled via the `API-Version` header. Breaking changes will increment the major version number.

## Generating Client Code

### TypeScript/JavaScript

```bash
npx @openapitools/openapi-generator-cli generate \
  -i docs/dev/api/openapi.yaml \
  -g typescript-axios \
  -o ./generated/typescript-client
```

### Rust

```bash
openapi-generator generate \
  -i docs/dev/api/openapi.yaml \
  -g rust \
  -o ./generated/rust-client
```

### Python

```bash
openapi-generator generate \
  -i docs/dev/api/openapi.yaml \
  -g python \
  -o ./generated/python-client
```

## API Version Headers

All API responses include an `API-Version` header indicating the API version:

```text
API-Version: 1.0.0
```

This header is added automatically by the `api_version_middleware` and helps clients track which API version they're using.

## References

- [OpenAPI Specification](https://swagger.io/specification/)
- [OpenAPI Tools](https://openapi.tools/)
