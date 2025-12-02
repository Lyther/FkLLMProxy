import Fastify from 'fastify';
import { browser } from './browser.js';
import { session } from './session.js';
import { tokens } from './tokens.js';

const DEFAULT_PORT = 3001;
const DEFAULT_HOST = '127.0.0.1';

const server = Fastify({
  logger: {
    transport: {
      target: 'pino-pretty',
      options: {
        translateTime: 'HH:MM:ss Z',
        ignore: 'pid,hostname',
      },
    },
  },
});

server.get('/health', async (request, reply) => {
  const browserAlive = browser.isAlive();
  const sessionValid = await session.isValid();
  const lastTokenRefresh = tokens.getLastRefreshTime();

  return {
    browser_alive: browserAlive,
    session_valid: sessionValid,
    last_token_refresh: lastTokenRefresh,
  };
});

server.get('/tokens', async (request, reply) => {
  try {
    const tokenData = await tokens.getTokens();
    return tokenData;
  } catch (error) {
    const errorMessage = error instanceof Error ? error.message : String(error);
    server.log.error(`Failed to get tokens: ${errorMessage}`);
    reply.code(503);
    return { error: errorMessage };
  }
});

server.post('/refresh', async (request, reply) => {
  const body = request.body as { force_arkose?: boolean };
  const forceArkose = body?.force_arkose ?? false;

  try {
    const tokenData = await tokens.refreshTokens(forceArkose);
    return tokenData;
  } catch (error) {
    const errorMessage = error instanceof Error ? error.message : String(error);
    server.log.error(`Failed to refresh tokens: ${errorMessage}`);
    reply.code(500);
    return { error: errorMessage };
  }
});

const start = async () => {
  try {
    await browser.initialize();
    await session.initialize();

    const port = parseInt(process.env.PORT || String(DEFAULT_PORT), 10);
    const host = process.env.HOST || DEFAULT_HOST;

    await server.listen({ port, host });
    server.log.info(`Harvester listening on ${host}:${port}`);
  } catch (err) {
    const errorMessage = err instanceof Error ? err.message : String(err);
    server.log.error(`Failed to start harvester: ${errorMessage}`);
    process.exit(1);
  }
};

start();
