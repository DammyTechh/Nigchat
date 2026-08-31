import * as SecureStore from 'expo-secure-store';

/**
 * HTTP client for the NigChat backend.
 *
 * Three behaviours matter here, and all three exist because of what mobile
 * networks actually do:
 *
 *  1. **Automatic refresh on 401.** Access tokens last 15 minutes. Every screen
 *     would otherwise need to handle expiry, and one that forgot would sign the
 *     user out at random.
 *  2. **Single-flight refresh.** When six requests hit a stale token at once,
 *     only one refresh goes out. Without this, five of them rotate a token that
 *     has already been rotated — which the backend correctly treats as theft
 *     and revokes the entire device.
 *  3. **Timeouts.** A request with no deadline hangs forever on a dying
 *     connection, and the UI spins with nothing to show.
 */

const API_URL = process.env.EXPO_PUBLIC_API_URL ?? 'http://localhost:8080';
const TIMEOUT_MS = 15_000;

const ACCESS_KEY = 'nigchat.access-token';
const REFRESH_KEY = 'nigchat.refresh-token';
const USER_KEY = 'nigchat.user-id';
const DEVICE_KEY = 'nigchat.device-id';

export interface ApiErrorShape {
  code: string;
  message: string;
  retry_after_seconds?: number;
}

export class ApiError extends Error {
  code: string;
  status: number;
  retryAfter?: number;

  constructor(status: number, body: ApiErrorShape) {
    super(body.message);
    this.name = 'ApiError';
    this.status = status;
    this.code = body.code;
    this.retryAfter = body.retry_after_seconds;
  }

  /** True when retrying the identical request could succeed. */
  get isRetryable() {
    return this.status >= 500 || this.code === 'rate_limited';
  }
}

export const tokens = {
  async get() {
    const [access, refresh, userId, deviceId] = await Promise.all([
      SecureStore.getItemAsync(ACCESS_KEY),
      SecureStore.getItemAsync(REFRESH_KEY),
      SecureStore.getItemAsync(USER_KEY),
      SecureStore.getItemAsync(DEVICE_KEY),
    ]);
    return { access, refresh, userId, deviceId };
  },

  async save(pair: {
    access_token: string;
    refresh_token: string;
    user_id: string;
    device_id: string;
  }) {
    // SecureStore is the Keychain on iOS and EncryptedSharedPreferences on
    // Android. Tokens never touch AsyncStorage, which is plain text on disk.
    await Promise.all([
      SecureStore.setItemAsync(ACCESS_KEY, pair.access_token),
      SecureStore.setItemAsync(REFRESH_KEY, pair.refresh_token),
      SecureStore.setItemAsync(USER_KEY, pair.user_id),
      SecureStore.setItemAsync(DEVICE_KEY, pair.device_id),
    ]);
  },

  async clear() {
    await Promise.all([
      SecureStore.deleteItemAsync(ACCESS_KEY),
      SecureStore.deleteItemAsync(REFRESH_KEY),
      SecureStore.deleteItemAsync(USER_KEY),
      // The device id deliberately survives sign-out: presenting it on the next
      // sign-in reuses the device row instead of littering the user's linked
      // devices list with a new entry per reinstall.
    ]);
  },
};

/** Set by the auth store so a hard sign-out can be triggered from anywhere. */
let onUnauthorized: (() => void) | null = null;
export function setUnauthorizedHandler(handler: () => void) {
  onUnauthorized = handler;
}

let refreshInFlight: Promise<string | null> | null = null;

async function refreshAccessToken(): Promise<string | null> {
  // Single-flight: concurrent callers await the same promise.
  if (refreshInFlight) return refreshInFlight;

  refreshInFlight = (async () => {
    try {
      const { refresh } = await tokens.get();
      if (!refresh) return null;

      const response = await fetch(`${API_URL}/v1/auth/refresh`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ refresh_token: refresh }),
      });

      if (!response.ok) {
        await tokens.clear();
        onUnauthorized?.();
        return null;
      }

      const pair = await response.json();
      await tokens.save(pair);
      return pair.access_token as string;
    } catch {
      // Network failure is not an auth failure — keep the tokens and let the
      // caller surface an offline state instead of signing the user out.
      return null;
    } finally {
      refreshInFlight = null;
    }
  })();

  return refreshInFlight;
}

interface RequestOptions {
  method?: 'GET' | 'POST' | 'PATCH' | 'PUT' | 'DELETE';
  body?: unknown;
  /** Skips the Authorization header — used by the auth endpoints themselves. */
  anonymous?: boolean;
  signal?: AbortSignal;
}

async function request<T>(path: string, options: RequestOptions = {}, retrying = false): Promise<T> {
  const { method = 'GET', body, anonymous, signal } = options;

  const headers: Record<string, string> = { 'Content-Type': 'application/json' };
  if (!anonymous) {
    const { access } = await tokens.get();
    if (access) headers.Authorization = `Bearer ${access}`;
  }

  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), TIMEOUT_MS);
  signal?.addEventListener('abort', () => controller.abort());

  let response: Response;
  try {
    response = await fetch(`${API_URL}${path}`, {
      method,
      headers,
      body: body === undefined ? undefined : JSON.stringify(body),
      signal: controller.signal,
    });
  } catch (error) {
    clearTimeout(timeout);
    if ((error as Error).name === 'AbortError') {
      throw new ApiError(0, { code: 'timeout', message: 'The request timed out.' });
    }
    throw new ApiError(0, {
      code: 'offline',
      message: 'No connection. Check your network and try again.',
    });
  }
  clearTimeout(timeout);

  if (response.status === 401 && !anonymous && !retrying) {
    const refreshed = await refreshAccessToken();
    if (refreshed) return request<T>(path, options, true);
    onUnauthorized?.();
  }

  if (!response.ok) {
    let payload: ApiErrorShape = { code: 'unknown', message: 'Something went wrong.' };
    try {
      const parsed = await response.json();
      if (parsed?.error) payload = parsed.error;
    } catch {
      // A non-JSON error body (a proxy 502, say) keeps the generic message.
    }
    throw new ApiError(response.status, payload);
  }

  if (response.status === 204) return undefined as T;
  return (await response.json()) as T;
}

export const api = {
  get: <T>(path: string, options?: RequestOptions) => request<T>(path, { ...options, method: 'GET' }),
  post: <T>(path: string, body?: unknown, options?: RequestOptions) =>
    request<T>(path, { ...options, method: 'POST', body }),
  patch: <T>(path: string, body?: unknown) => request<T>(path, { method: 'PATCH', body }),
  put: <T>(path: string, body?: unknown) => request<T>(path, { method: 'PUT', body }),
  delete: <T>(path: string, body?: unknown) => request<T>(path, { method: 'DELETE', body }),
  url: API_URL,
};
