FROM node:22-alpine AS builder

WORKDIR /app

# Update packages and install build dependencies for TypeScript compilation
RUN apk update && apk upgrade && apk add --no-cache \
    python3 \
    make \
    g++ \
    && rm -rf /var/cache/apk/*

# Copy package files
COPY package.json package-lock.json* tsconfig.json ./
RUN npm ci || npm install

# Copy source files
COPY src ./src

# Build TypeScript
RUN npm run build

FROM gcr.io/distroless/nodejs22-debian12:nonroot

WORKDIR /app

COPY --from=builder /app/dist ./dist
COPY --from=builder /app/package.json ./
COPY --from=builder /app/node_modules ./node_modules

EXPOSE 4001

ENV NODE_ENV=production
ENV PORT=4001
ENV HOST=0.0.0.0

CMD ["dist/index.js"]

