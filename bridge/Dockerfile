# Multi-stage build for anthropic-bridge (Node.js/TypeScript)
FROM node:20.11-alpine3.19 AS builder

WORKDIR /app

# Install build dependencies
RUN apk add --no-cache \
    python3 \
    make \
    g++

# Copy package files
COPY package.json package-lock.json* tsconfig.json ./
# Regenerate lock file if missing or use npm install as fallback for out-of-sync lock files
RUN if [ -f package-lock.json ]; then \
      npm ci || (echo "Lock file out of sync, regenerating..." && rm -f package-lock.json && npm install); \
    else \
      npm install; \
    fi

# Copy source files
COPY src ./src

# Build TypeScript
RUN npm run build

# Runtime image
FROM node:20.11-alpine3.19 AS runner

WORKDIR /app

# Install curl for healthcheck
RUN apk add --no-cache curl

# Use existing node user (UID/GID 1000) - Alpine node image already has it
# Ensure /app is owned by node user
RUN mkdir -p /app && \
    chown -R node:node /app

# Copy built artifacts
COPY --from=builder --chown=node:node /app/dist ./dist
COPY --from=builder --chown=node:node /app/package.json ./
COPY --from=builder --chown=node:node /app/node_modules ./node_modules

USER node

EXPOSE 4001

ENV NODE_ENV=production
ENV PORT=4001
ENV HOST=0.0.0.0

HEALTHCHECK --interval=30s --timeout=10s --start-period=30s --retries=3 \
    CMD curl -f http://localhost:4001/health || exit 1

CMD ["node", "dist/index.js"]
