import { chromium, Browser, BrowserContext } from 'playwright';

let browserInstance: Browser | null = null;
let context: BrowserContext | null = null;

export const browser = {
  async initialize(): Promise<void> {
    if (browserInstance) {
      return;
    }

    browserInstance = await chromium.launch({
      headless: true,
      args: ['--disable-blink-features=AutomationControlled'],
    });

    context = await browserInstance.newContext({
      userAgent:
        'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36',
      viewport: { width: 1920, height: 1080 },
    });

    await context.addCookies([]);
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

