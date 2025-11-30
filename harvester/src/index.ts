import Fastify from 'fastify';
import { browser } from './browser.js';
import { session } from './session.js';
import { tokens } from './tokens.js';
import { arkose } from './arkose.js';

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
    reply.code(503);
    return { error: error instanceof Error ? error.message : 'Unknown error' };
  }
});

server.post('/refresh', async (request, reply) => {
  const body = request.body as { force_arkose?: boolean };
  const forceArkose = body?.force_arkose ?? false;

  try {
    const tokenData = await tokens.refreshTokens(forceArkose);
    return tokenData;
  } catch (error) {
    reply.code(500);
    return { error: error instanceof Error ? error.message : 'Unknown error' };
  }
});

const start = async () => {
  try {
    await browser.initialize();
    await session.initialize();

    const port = parseInt(process.env.PORT || '3001', 10);
    const host = process.env.HOST || '127.0.0.1';

    await server.listen({ port, host });
    server.log.info(`Harvester listening on ${host}:${port}`);
  } catch (err) {
    server.log.error(err);
    process.exit(1);
  }
};

start();

