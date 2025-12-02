import { browser } from './browser.js';

const ARKOSE_TOKEN_TTL_MS = 120 * 1000;
const ARKOSE_TIMEOUT_MS = 10000;

let cachedToken: string | null = null;
let tokenExpiry = 0;

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

          setTimeout(() => reject(new Error('Arkose timeout')), ARKOSE_TIMEOUT_MS);
        });
      });

      cachedToken = token;
      tokenExpiry = now + ARKOSE_TOKEN_TTL_MS;

      return token;
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      const cause = error instanceof Error && error.cause ? error.cause : undefined;
      const enhancedError = new Error(`Failed to generate Arkose token: ${errorMessage}`, { cause });
      throw enhancedError;
    } finally {
      await page.close();
    }
  },
};
