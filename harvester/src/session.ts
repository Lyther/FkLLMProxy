import * as fs from 'fs/promises';
import * as path from 'path';
import { browser } from './browser.js';
import { logger } from './logger.js';

const COOKIES_FILE = path.join(process.cwd(), 'cookies.json');
const INITIALIZATION_WAIT_MS = 2000;
const KEEP_ALIVE_WAIT_MS = 1000;
const KEEP_ALIVE_INTERVAL_MS = 5 * 60 * 1000;

interface Cookie {
  name: string;
  value: string;
  domain?: string;
  path?: string;
  expires?: number;
  httpOnly?: boolean;
  secure?: boolean;
  sameSite?: 'Strict' | 'Lax' | 'None';
}

let accessToken: string | null = null;
let sessionValid = false;
let keepAliveInterval: NodeJS.Timeout | null = null;

async function loadCookies(): Promise<Cookie[]> {
  try {
    const data = await fs.readFile(COOKIES_FILE, 'utf-8');
    const parsed = JSON.parse(data);
    return Array.isArray(parsed) ? parsed : [];
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code !== 'ENOENT') {
      logger.error({ err: error }, 'Failed to load cookies');
    }
    return [];
  }
}

async function saveCookies(cookies: Cookie[]): Promise<void> {
  try {
    await fs.writeFile(COOKIES_FILE, JSON.stringify(cookies, null, 2));
  } catch (error) {
    logger.error({ err: error }, 'Failed to save cookies');
  }
}

export const session = {
  async initialize(): Promise<void> {
    const context = browser.getContext();
    const savedCookies = await loadCookies();

    if (savedCookies.length > 0) {
      await context.addCookies(savedCookies);
    }

    const page = await context.newPage();

    try {
      await page.goto('https://chatgpt.com/', { waitUntil: 'networkidle' });

      page.on('response', async (response) => {
        const url = response.url();
        if (url.includes('/api/auth/session')) {
          try {
            const data = await response.json();
            if (data.accessToken) {
              accessToken = data.accessToken;
              sessionValid = true;
            }
          } catch (e) {
            logger.error({ err: e }, 'Failed to parse session response JSON');
          }
        }
      });

      await page.waitForTimeout(INITIALIZATION_WAIT_MS);

      const cookies = await context.cookies();
      await saveCookies(cookies);

      this.startKeepAlive();
    } catch (error) {
      logger.error({ err: error }, 'Session initialization error');
      throw error;
    } finally {
      await page.close();
    }
  },

  startKeepAlive(): void {
    if (keepAliveInterval) {
      clearInterval(keepAliveInterval);
    }

    keepAliveInterval = setInterval(async () => {
      try {
        const context = browser.getContext();
        const page = await context.newPage();
        await page.goto('https://chatgpt.com/', { waitUntil: 'domcontentloaded' });
        await page.waitForTimeout(KEEP_ALIVE_WAIT_MS);

        const cookies = await context.cookies();
        await saveCookies(cookies);

        await page.close();
      } catch (error) {
        logger.error({ err: error }, 'Keep-alive error');
      }
    }, KEEP_ALIVE_INTERVAL_MS);
  },

  stopKeepAlive(): void {
    if (keepAliveInterval) {
      clearInterval(keepAliveInterval);
      keepAliveInterval = null;
    }
  },

  async isValid(): Promise<boolean> {
    if (!accessToken) {
      return false;
    }

    try {
      const context = browser.getContext();
      const page = await context.newPage();
      const response = await page.goto('https://chatgpt.com/api/auth/session', {
        waitUntil: 'networkidle',
      });

      if (response?.status() === 200) {
        const data = await response.json();
        if (data.accessToken) {
          accessToken = data.accessToken;
          sessionValid = true;
          await page.close();
          return true;
        }
      }

      await page.close();
      return false;
    } catch (error) {
      logger.error({ err: error }, 'Session validation error');
      return false;
    }
  },

  getAccessToken(): string | null {
    return accessToken;
  },
};
