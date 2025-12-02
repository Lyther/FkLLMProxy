import { arkose } from './arkose.js';
import { session } from './session.js';

interface TokenData {
  access_token: string;
  arkose_token?: string;
  expires_at: number;
}

const ACCESS_TOKEN_TTL_MS = 3600 * 1000;
const ARKOSE_TOKEN_TTL_MS = 120 * 1000;
const ACCESS_TOKEN_TTL_SECS = 3600;

let cachedTokens: TokenData | null = null;
let lastRefreshTime = 0;

export const tokens = {
  async getTokens(): Promise<TokenData> {
    const now = Date.now();

    if (cachedTokens) {
      const age = now - lastRefreshTime;
      const ttl = cachedTokens.arkose_token ? ARKOSE_TOKEN_TTL_MS : ACCESS_TOKEN_TTL_MS;

      if (age < ttl) {
        return cachedTokens;
      }
    }

    return this.refreshTokens(false);
  },

  async refreshTokens(forceArkose: boolean): Promise<TokenData> {
    const accessToken = session.getAccessToken();
    if (!accessToken) {
      throw new Error('No access token available - session not initialized');
    }

    let arkoseToken: string | undefined;
    if (forceArkose || !cachedTokens?.arkose_token) {
      // Propagate Arkose errors when explicitly required (GPT-4 models)
      arkoseToken = await arkose.getToken();
    } else {
      arkoseToken = cachedTokens.arkose_token;
    }

    const expiresAt = Math.floor(Date.now() / 1000) + ACCESS_TOKEN_TTL_SECS;

    cachedTokens = {
      access_token: accessToken,
      arkose_token: arkoseToken,
      expires_at: expiresAt,
    };

    lastRefreshTime = Date.now();
    return cachedTokens;
  },

  getLastRefreshTime(): number {
    return Math.floor(lastRefreshTime / 1000);
  },
};
