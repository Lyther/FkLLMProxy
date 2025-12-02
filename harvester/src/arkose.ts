import { browser } from './browser.js';

let cachedToken: string | null = null;
let tokenExpiry = 0;

const TOKEN_TTL = 120 * 1000;

export const arkose = {
  async getToken(): Promise<string> {
    const now = Date.now();
    if (cachedToken && now < tokenExpiry) {
      return cachedToken;
    }

    const context = browser.getContext();
    const page = await context.newPage();

    try {
      await page.goto('https://chatgpt.com/', { waitUntil: 'networkidle' });

      const token = await page.evaluate(() => {
        return new Promise<string>((resolve, reject) => {
          // page.evaluate() runs in browser context where window exists
          // Type assertion for browser globals available in evaluate context
          interface ArkoseAPI {
            runEnforcement: (callback: (data: { token?: string }) => void) => void;
          }

          const globalWindow = globalThis as typeof globalThis & {
            arkose?: ArkoseAPI;
          };

          if (typeof globalWindow !== 'undefined' && globalWindow.arkose) {
            try {
              globalWindow.arkose.runEnforcement((data: { token?: string }) => {
                if (data.token) {
                  resolve(data.token);
                } else {
                  reject(new Error('No token in Arkose response'));
                }
              });
            } catch (error) {
              reject(error);
            }
          } else {
            reject(new Error('Arkose not available'));
          }

          setTimeout(() => reject(new Error('Arkose timeout')), 10000);
        });
      });

      cachedToken = token;
      tokenExpiry = now + TOKEN_TTL;

      return token;
    } catch (error) {
      console.error('Arkose token generation error:', error);
      throw new Error(`Failed to generate Arkose token: ${error}`);
    } finally {
      await page.close();
    }
  },
};

