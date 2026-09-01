/**
 * HTTP client.
 *
 * Mirrors the mobile client's behaviour, including single-flight refresh: when
 * several requests hit a stale access token at once, only one refresh goes out.
 * Without that, the others rotate an already-rotated token, which the backend
 * correctly treats as theft and revokes the whole device — signing the user out
 * of a browser they were actively using.
 *
 * Tokens live in `localStorage`. That is a real, acknowledged trade-off: it is
 * readable by any script that achieves XSS on this origin. The mitigations are
 * a strict CSP, no third-party scripts, short-lived access tokens, and a
 * refresh token bound to this device that the user can revoke from their phone.
 * The alternative — httpOnly cookies — would require the API to become
 * cookie-aware and to carry CSRF protection, which is a larger change than it
 * looks.
 */

const API_URL = import.meta.env.VITE_API_URL ?? '';
const TIMEOUT_MS = 15_000;

const ACCESS_KEY = 'nigchat.access';
const REFRESH_KEY = 'nigchat.refresh';
const USER_KEY = 'nigchat.user';
const DEVICE_KEY = 'nigchat.device';

export interface ApiErrorBody {
  code: string;
  message: string;
  retry_after_seconds?: number;
}

export class ApiError extends Error {
  code: string;
  status: number;
  retryAfter?: number;

  constructor(status: number, body: ApiErrorBody) {
    super(body.message);
    this.name = 'ApiError';
    this.status = status;
    this.code = body.code;
    this.retryAfter = body.retry_after_seconds;
  }
}

export const session = {
  get access() {
    return localStorage.getItem(ACCESS_KEY);
  },
  get refresh() {
    return localStorage.getItem(REFRESH_KEY);
  },
  get userId() {
    return localStorage.getItem(USER_KEY);
  },
  get deviceId() {
    return localStorage.getItem(DEVICE_KEY);
  },
  save(pair: {
    access_token: string;
    refresh_token: string;
    user_id: string;
    device_id: string;
  }) {
    localStorage.setItem(ACCESS_KEY, pair.access_token);
    localStorage.setItem(REFRESH_KEY, pair.refresh_token);
    localStorage.setItem(USER_KEY, pair.user_id);
    localStorage.setItem(DEVICE_KEY, pair.device_id);
  },
  clear() {
    [ACCESS_KEY, REFRESH_KEY, USER_KEY, DEVICE_KEY].forEach((key) =>
      localStorage.removeItem(key),
    );
  },
};

let onUnauthorized: (() => void) | null = null;
export function setUnauthorizedHandler(handler: () => void) {
  onUnauthorized = handler;
}

let refreshInFlight: Promise<string | null> | null = null;

async function refreshAccessToken(): Promise<string | null> {
  if (refreshInFlight) return refreshInFlight;

  refreshInFlight = (async () => {
    try {
      const refresh = session.refresh;
      if (!refresh) return null;

      const response = await fetch(`${API_URL}/v1/auth/refresh`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ refresh_token: refresh }),
      });

      if (!response.ok) {
        session.clear();
        onUnauthorized?.();
        return null;
      }

      const pair = await response.json();
      session.save(pair);
      return pair.access_token as string;
    } catch {
      // A network failure is not an auth failure. Keep the tokens; the UI shows
      // an offline state instead of throwing the user out.
      return null;
    } finally {
      refreshInFlight = null;
    }
  })();

  return refreshInFlight;
}

interface Options {
  method?: 'GET' | 'POST' | 'PATCH' | 'PUT' | 'DELETE';
  body?: unknown;
  anonymous?: boolean;
}

async function request<T>(path: string, options: Options = {}, retrying = false): Promise<T> {
  const { method = 'GET', body, anonymous } = options;

  const headers: Record<string, string> = { 'Content-Type': 'application/json' };
  if (!anonymous && session.access) headers.Authorization = `Bearer ${session.access}`;

  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), TIMEOUT_MS);

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
    const timedOut = (error as Error).name === 'AbortError';
    throw new ApiError(0, {
      code: timedOut ? 'timeout' : 'offline',
      message: timedOut ? 'The request timed out.' : 'No connection to NigChat.',
    });
  }
  clearTimeout(timeout);

  if (response.status === 401 && !anonymous && !retrying) {
    const refreshed = await refreshAccessToken();
    if (refreshed) return request<T>(path, options, true);
    onUnauthorized?.();
  }

  if (!response.ok) {
    let payload: ApiErrorBody = { code: 'unknown', message: 'Something went wrong.' };
    try {
      const parsed = await response.json();
      if (parsed?.error) payload = parsed.error;
    } catch {
      /* non-JSON error body (a proxy 502) keeps the generic message */
    }
    throw new ApiError(response.status, payload);
  }

  if (response.status === 204) return undefined as T;
  return (await response.json()) as T;
}

export const api = {
  get: <T,>(path: string, options?: Options) => request<T>(path, options),
  post: <T,>(path: string, body?: unknown, options?: Options) =>
    request<T>(path, { ...options, method: 'POST', body }),
  patch: <T,>(path: string, body?: unknown) => request<T>(path, { method: 'PATCH', body }),
  delete: <T,>(path: string) => request<T>(path, { method: 'DELETE' }),
  url: API_URL,
};
