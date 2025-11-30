# Local Testing with Act

This guide explains how to test GitHub Actions workflows locally using [act](https://github.com/nektos/act).

## Installation

```bash
# macOS
brew install act

# Linux
curl https://raw.githubusercontent.com/nektos/act/master/install.sh | sudo bash

# Or download from releases: https://github.com/nektos/act/releases
```

## Docker Hub Credentials

To test the release workflow with Docker Hub push, you need to configure secrets.

### Option 1: Using .actrc file

Create a `.actrc` file in the project root:

```bash
-s DOCKER_USERNAME=your-dockerhub-username
-s DOCKER_PASSWORD=your-dockerhub-password
```

Then run:
```bash
act -W .github/workflows/release.yml -j docker
```

### Option 2: Using command-line flags

```bash
act -W .github/workflows/release.yml -j docker \
  -s DOCKER_USERNAME=your-username \
  -s DOCKER_PASSWORD=your-password
```

### Option 3: Using environment variables

```bash
export ACT_SECRET_DOCKER_USERNAME=your-username
export ACT_SECRET_DOCKER_PASSWORD=your-password
act -W .github/workflows/release.yml -j docker
```

## Testing CI Workflow

For the CI workflow, you typically don't need secrets unless running E2E tests:

```bash
# Run all CI jobs
act -W .github/workflows/ci.yml

# Run specific job
act -W .github/workflows/ci.yml -j lint
act -W .github/workflows/ci.yml -j test-unit
```

## E2E Tests with Credentials

To run E2E tests locally:

```bash
act -W .github/workflows/ci.yml -j test-e2e \
  -s VERTEX_API_KEY=your-api-key \
  -s GOOGLE_APPLICATION_CREDENTIALS=/path/to/sa.json \
  -s GOOGLE_CLOUD_PROJECT=your-project-id
```

## Notes

- Artifact uploads will fail in `act` (expected) - they use `continue-on-error: true`
- Docker push is disabled by default in release workflow when testing locally
- Secrets are not persisted - they're only available during the act session
