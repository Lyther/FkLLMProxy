import { browser } from './browser.js';
import * as fs from 'fs';
import * as path from 'path';

const COOKIES_FILE = path.join(process.cwd(), 'cookies.json');

let accessToken: string | null = null;
let sessionValid = false;
let keepAliveInterval: NodeJS.Timeout | null = null;

function loadCookies(): any[] {
  try {
    if (fs.existsSync(COOKIES_FILE)) {
      const data = fs.readFileSync(COOKIES_FILE, 'utf-8');
      return JSON.parse(data);
    }
  } catch (error) {
    console.error('Failed to load cookies:', error);
  }
  return [];
}

function saveCookies(cookies: any[]): void {
  try {
    fs.writeFileSync(COOKIES_FILE, JSON.stringify(cookies, null, 2));
  } catch (error) {
    console.error('Failed to save cookies:', error);
  }
}

export const session = {
  async initialize(): Promise<void> {
    const context = browser.getContext();
    const savedCookies = loadCookies();

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
            // Ignore JSON parse errors
          }
        }
      });

      await page.waitForTimeout(2000);

      const cookies = await context.cookies();
      saveCookies(cookies);

      this.startKeepAlive();
    } catch (error) {
      console.error('Session initialization error:', error);
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
        await page.waitForTimeout(1000);

        const cookies = await context.cookies();
        saveCookies(cookies);

        await page.close();
      } catch (error) {
        console.error('Keep-alive error:', error);
      }
    }, 5 * 60 * 1000);
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
      console.error('Session validation error:', error);
      return false;
    }
  },

  getAccessToken(): string | null {
    return accessToken;
  },
};

