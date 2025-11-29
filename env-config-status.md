# [02-BUILD] Env Config (The Secrets)

## Status: ✅ Completed

### 1. The Schema (Type Safety)

- **Implemented**: `src/config/mod.rs` uses `validator` crate to enforce rules.
- **Constraints**:
  - `server.port`: Validated range (1-65535).
  - `auth.master_key`: Must be non-empty if auth is enabled.
  - **Logic**: App crashes if neither `GOOGLE_API_KEY` nor `GOOGLE_APPLICATION_CREDENTIALS` is present.

### 2. Leak Prevention

- **Action**: Secrets removed from `vertex-bridge.toml`.
- **Action**: `vertex-bridge.toml` now only contains defaults.
- **Action**: Secrets moved to `.env` (which is gitignored).

### 3. The Quarantine

- **Action**: `.gitignore` updated to exclude `.env` and `service-account.json`.
- **Action**: `.env.example` created with placeholders.

### 4. Access Control

- **Constraint**: `std::env::var` usage restricted to `src/config/mod.rs` (and `main.rs` for initial load).
- **Refactor**: `TokenManager` now receives credentials path from config, decoupling it from environment variables.

## 📦 Deliverables

- `src/config/mod.rs`: Validated config struct.
- `.env`: Contains real secrets (local only).
- `.env.example`: Safe template.
- `.gitignore`: Updated rules.
