import { Browser, BrowserContext, chromium } from 'playwright';

const DEFAULT_USER_AGENT = 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36';
const DEFAULT_VIEWPORT = { width: 1920, height: 1080 };

let browserInstance: Browser | null = null;
let context: BrowserContext | null = null;

export const browser = {
  async initialize(): Promise<void> {
    if (browserInstance) {
      return;
    }

    try {
      browserInstance = await chromium.launch({
        headless: true,
        args: ['--disable-blink-features=AutomationControlled'],
      });

      context = await browserInstance.newContext({
        userAgent: process.env.USER_AGENT || DEFAULT_USER_AGENT,
        viewport: process.env.VIEWPORT_WIDTH && process.env.VIEWPORT_HEIGHT
          ? { width: parseInt(process.env.VIEWPORT_WIDTH), height: parseInt(process.env.VIEWPORT_HEIGHT) }
          : DEFAULT_VIEWPORT,
      });

      await context.addCookies([]);
    } catch (error) {
      if (context) {
        await context.close().catch(() => { });
        context = null;
      }
      if (browserInstance) {
        await browserInstance.close().catch(() => { });
        browserInstance = null;
      }
      throw error;
    }
  },

  getContext(): BrowserContext {
    if (!context) {
      throw new Error('Browser context not initialized');
    }
    return context;
  },

  isAlive(): boolean {
    return browserInstance !== null && browserInstance.isConnected();
  },

  async close(): Promise<void> {
    const { session } = await import('./session.js');
    session.stopKeepAlive();

    if (context) {
      await context.close();
      context = null;
    }
    if (browserInstance) {
      await browserInstance.close();
      browserInstance = null;
    }
  },
};
