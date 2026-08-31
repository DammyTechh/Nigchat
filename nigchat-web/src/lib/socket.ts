import type { ServerEvent } from './types';
import { api, session } from './api';

/**
 * Realtime connection.
 *
 * Same contract as the mobile client: the socket is a fast path, never the
 * source of truth. Every reconnect re-syncs from the last known `seq`, so a
 * dropped connection can delay a message but not lose one.
 *
 * The browser adds one case the phone does not have — a backgrounded tab. Most
 * browsers throttle timers in hidden tabs, so a socket can sit apparently open
 * for minutes while nothing arrives. Reconnection is therefore driven by the
 * `visibilitychange` and `online` events as well as by the heartbeat.
 */

type Listener = (event: ServerEvent) => void;
export type SocketStatus = 'connecting' | 'online' | 'offline';

const WS_URL =
  import.meta.env.VITE_WS_URL ??
  `${location.protocol === 'https:' ? 'wss' : 'ws'}://${location.host}`;

const BACKOFF = [1_000, 2_000, 5_000, 10_000, 20_000, 30_000];

class RealtimeSocket {
  private socket: WebSocket | null = null;
  private listeners = new Set<Listener>();
  private statusListeners = new Set<(status: SocketStatus) => void>();
  private attempt = 0;
  private reconnectTimer: number | null = null;
  private heartbeatTimer: number | null = null;
  private lastMessageAt = Date.now();
  private closedByUs = false;
  private status: SocketStatus = 'offline';

  connect() {
    this.closedByUs = false;
    const token = session.access;
    if (!token) return;

    this.cleanup();
    this.setStatus('connecting');

    const url = `${WS_URL || api.url}/v1/ws?token=${encodeURIComponent(token)}`;
    const socket = new WebSocket(url);
    this.socket = socket;

    socket.onopen = () => {
      this.attempt = 0;
      this.lastMessageAt = Date.now();
      this.setStatus('online');
      this.startHeartbeat();
    };

    socket.onmessage = (event) => {
      this.lastMessageAt = Date.now();
      try {
        const parsed = JSON.parse(event.data) as ServerEvent;
        this.listeners.forEach((listener) => listener(parsed));
      } catch {
        /* an unparseable frame is a server bug; dropping it beats killing the
           connection that carries everything else */
      }
    };

    socket.onclose = () => {
      this.setStatus('offline');
      this.stopHeartbeat();
      if (!this.closedByUs) this.scheduleReconnect();
    };
  }

  private scheduleReconnect() {
    if (this.reconnectTimer) return;
    const delay = BACKOFF[Math.min(this.attempt, BACKOFF.length - 1)];
    this.attempt += 1;

    // Jitter stops every open tab reconnecting in lockstep and hammering the
    // server the moment it recovers.
    this.reconnectTimer = window.setTimeout(
      () => {
        this.reconnectTimer = null;
        this.connect();
      },
      delay + Math.random() * 400,
    );
  }

  private startHeartbeat() {
    this.stopHeartbeat();
    this.heartbeatTimer = window.setInterval(() => {
      if (Date.now() - this.lastMessageAt > 70_000) {
        this.socket?.close();
        return;
      }
      this.send({ type: 'ping' });
    }, 25_000);
  }

  private stopHeartbeat() {
    if (this.heartbeatTimer) window.clearInterval(this.heartbeatTimer);
    this.heartbeatTimer = null;
  }

  private cleanup() {
    if (this.reconnectTimer) window.clearTimeout(this.reconnectTimer);
    this.reconnectTimer = null;
    this.stopHeartbeat();
    if (this.socket) {
      this.socket.onclose = null;
      this.socket.close();
      this.socket = null;
    }
  }

  private setStatus(status: SocketStatus) {
    if (this.status === status) return;
    this.status = status;
    this.statusListeners.forEach((listener) => listener(status));
  }

  send(payload: unknown) {
    if (this.socket?.readyState === WebSocket.OPEN) {
      this.socket.send(JSON.stringify(payload));
    }
  }

  sendTyping(conversationId: string, state: 'typing' | 'stopped') {
    this.send({ type: 'typing', data: { conversation_id: conversationId, state } });
  }

  subscribe(listener: Listener) {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  onStatus(listener: (status: SocketStatus) => void) {
    this.statusListeners.add(listener);
    listener(this.status);
    return () => this.statusListeners.delete(listener);
  }

  disconnect() {
    this.closedByUs = true;
    this.cleanup();
    this.setStatus('offline');
  }

  getStatus() {
    return this.status;
  }

  /** Nudge after the tab wakes or the network returns. */
  ensureConnected() {
    if (this.status === 'offline' && !this.closedByUs) {
      this.attempt = 0;
      this.connect();
    }
  }
}

export const socket = new RealtimeSocket();

// A throttled background tab can leave the socket apparently open while
// nothing flows. Re-check the moment the user comes back.
document.addEventListener('visibilitychange', () => {
  if (document.visibilityState === 'visible') socket.ensureConnected();
});
window.addEventListener('online', () => socket.ensureConnected());
