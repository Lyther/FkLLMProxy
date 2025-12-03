# Changelog

All notable changes to this project will be documented in this file. See [standard-version](https://github.com/conventional-changelog/standard-version) for commit guidelines.

## [1.0.2] - 2025-12-02

### 🐛 Bug Fixes

* **clippy**: Resolve all clippy warnings ([0f8bf44](https://github.com/Lyther/FkLLMProxy/commit/0f8bf44))
  * Collapse nested if statements
  * Remove redundant closures
  * Remove unnecessary casts
  * Use Option::map instead of manual implementation
  * Use cloned() instead of map(|k| k.clone())
  * Wrap unsafe env var operations in unsafe blocks

### 🔄 Refactoring

* **providers**: Consolidate provider implementations and handlers ([ddb5813](https://github.com/Lyther/FkLLMProxy/commit/ddb5813))
* **services**: Update harvester and bridge implementations ([16a6e94](https://github.com/Lyther/FkLLMProxy/commit/16a6e94))

### 🔧 Chores

* **security**: Configure gitleaks secret scanning ([c486770](https://github.com/Lyther/FkLLMProxy/commit/c486770))
* **deps**: Update dependencies and add npm scripts ([7885d7b](https://github.com/Lyther/FkLLMProxy/commit/7885d7b))
* **ci**: Add deployment workflow and pre-commit config ([edcccf9](https://github.com/Lyther/FkLLMProxy/commit/edcccf9))
* **infra**: Update Kubernetes and Terraform configs ([c243c10](https://github.com/Lyther/FkLLMProxy/commit/c243c10))
* **scripts**: Add deployment and testing scripts ([4ea8ed0](https://github.com/Lyther/FkLLMProxy/commit/4ea8ed0))
* **docker**: Update Dockerfile configurations ([bf250ca](https://github.com/Lyther/FkLLMProxy/commit/bf250ca))
* Update Makefile and add Cargo.lock ([5ba48d9](https://github.com/Lyther/FkLLMProxy/commit/5ba48d9))

### 📝 Documentation

* Remove archived and outdated documentation ([07cb281](https://github.com/Lyther/FkLLMProxy/commit/07cb281))
* Update documentation and configuration ([7682b21](https://github.com/Lyther/FkLLMProxy/commit/7682b21))

### 🎨 Style

* Apply rustfmt formatting fixes ([27513fe](https://github.com/Lyther/FkLLMProxy/commit/27513fe))

### 🧪 Tests

* Update integration test utilities ([7dde4ff](https://github.com/Lyther/FkLLMProxy/commit/7dde4ff))

## [1.0.1] - 2025-12-02

### 🐛 Bug Fixes

* Wait for assistant marker before processing output ([22a0da3](https://github.com/Lyther/FkLLMProxy/commit/22a0da35e04782a95f3f26b1a4b11a97119ff3d2))

### 🔧 Chores

* Update Dockerfiles, dependencies, and configuration files ([8e099c3](https://github.com/Lyther/FkLLMProxy/commit/8e099c3c8b097a21f8d3b33e6c40121b2d29ff7e))

## 1.0.0 (2025-11-30)

### Features

* add docker-compose and update docs ([6f59077](https://github.com/Lyther/FkLLMProxy/commit/6f590772f9cd6e23a5fc073dee82ff7cc4d1e4e5))
* v1.0 preview ([5810267](https://github.com/Lyther/FkLLMProxy/commit/58102674968ca471990b81fdf28bf4f5c24427e1))

### Bug Fixes

* **bridge:** bind to configured host instead of default ([f4e4c86](https://github.com/Lyther/FkLLMProxy/commit/f4e4c869e0e393554e8888ab0c87178843467b35))
* **deploy:** fix Docker build and config loading ([b0216b9](https://github.com/Lyther/FkLLMProxy/commit/b0216b9171324ed1a5edf8977c49ae1a88aaa515))
