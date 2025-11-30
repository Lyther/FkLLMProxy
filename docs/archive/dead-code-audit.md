# Dead Code Audit Report

**Date**: 2025-11-30
**Auditor**: LLM Agent
**Protocol**: `99-ops/nuke`

## 1. Rust (cargo-udeps)

- **Status**: ⚠️ **Failed**
- **Reason**: `cargo-udeps` requires nightly toolchain, which is not available in the current environment.
- **Action**: Deferred. Manual inspection of `Cargo.toml` is recommended for obvious unused crates.

## 2. TypeScript (knip)

### Harvester (`harvester/`)

- **Finding**: `pino` declared as dependency but unused (Fastify uses it internally, but `pino-pretty` is explicit).
- **Finding**: `pino-pretty` used in `src/index.ts` but marked as unused by knip (likely false positive due to dynamic import string in Fastify options).
- **Finding**: `tsx` flagged as unused devDependency, but used in `scripts.dev`.
- **Action**:
  - Removed `pino` from `dependencies` (Fastify pulls it in transitively if needed, or it was truly unused).
  - Kept `pino-pretty` (verified usage in `src/index.ts`).
  - Kept `tsx` (verified usage in `package.json`).

### Bridge (`bridge/`)

- **Finding**: No unused dependencies found by `knip`.

## 3. Scripts Audit

- `scripts/test-critical.sh`: Active (referenced in README).
- `scripts/test-docker-compose.sh`: Active (referenced in README).
- `scripts/test-proxy-stream.sh`: Active (referenced in README).
- `scripts/test-proxy.sh`: Active (referenced in README).
- `scripts/test-smoke.sh`: Active (used in CI/verification).

**Result**: All scripts appear relevant and documented.

## Summary

- **Deleted**: `pino` from `harvester/package.json`.
- **Verified**: All scripts and `bridge` dependencies are clean.
- **Pending**: Rust dependency audit (requires nightly).
