# Multi-stage build for harvester (Node.js/TypeScript with Playwright)
FROM node:20.11-bookworm-slim AS builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y \
    python3 \
    make \
    g++ \
    && rm -rf /var/lib/apt/lists/*

# Copy package files
COPY package.json package-lock.json* ./
# Regenerate lock file if missing or use npm install as fallback for out-of-sync lock files
RUN if [ -f package-lock.json ]; then \
    npm ci || (echo "Lock file out of sync, regenerating..." && rm -f package-lock.json && npm install); \
    else \
    npm install; \
    fi

# Copy source files
COPY tsconfig.json ./
COPY src ./src

# Build TypeScript
RUN npm run build

# Runtime image
FROM node:20.11-bookworm-slim AS runner

WORKDIR /app

# Install Playwright and system dependencies
RUN apt-get update && apt-get install -y \
    curl \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Set Playwright browser cache location to be accessible by node user
ENV PLAYWRIGHT_BROWSERS_PATH=/app/.cache/ms-playwright

# Copy built artifacts first (need node_modules for playwright version)
COPY --from=builder /app/dist ./dist
COPY --from=builder /app/package.json ./
COPY --from=builder /app/node_modules ./node_modules

# Install Playwright browsers using the version from node_modules (not npx latest)
RUN ./node_modules/.bin/playwright install chromium --with-deps || npx playwright install chromium --with-deps || true

# Use existing node user (UID/GID 1000) - Debian node image already has it
# Ensure /app and browser cache are owned by node user
RUN mkdir -p /app/.cache/ms-playwright && \
    chown -R node:node /app

USER node

EXPOSE 3001

ENV NODE_ENV=production
ENV PORT=3001
ENV HOST=0.0.0.0

HEALTHCHECK --interval=30s --timeout=10s --start-period=60s --retries=3 \
    CMD curl -f http://localhost:3001/health || exit 1

CMD ["node", "dist/index.js"]
