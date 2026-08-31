import type { ServerEvent } from './types';
import { api, tokens } from './client';

/**
 * Realtime connection.
 *
 * The socket is a *fast path*, never the source of truth. Every reconnect
 * triggers a re-sync from the last known `seq`, so a dropped connection can
 * only ever delay a message, never lose one. Building it the other way round —
 * trusting the socket to deliver everything — is the single most common way
 * chat clients end up with missing messages that nobody can reproduce.
 */

type Listener = (event: ServerEvent) => void;
type StatusListener = (status: SocketStatus) => void;
export type SocketStatus = 'connecting' | 'online' | 'offline';

const WS_URL = process.env.EXPO_PUBLIC_WS_URL ?? api.url.replace(/^http/, 'ws');

/** Backoff schedule in ms, with the last value repeating. */
const BACKOFF = [1_000, 2_000, 5_000, 10_000, 20_000, 30_000];

class RealtimeSocket {
  private socket: WebSocket | null = null;
  private listeners = new Set<Listener>();
  private statusListeners = new Set<StatusListener>();
  private attempt = 0;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private heartbeatTimer: ReturnType<typeof setInterval> | null = null;
  private lastPong = Date.now();
  private closedByUs = false;
  private status: SocketStatus = 'offline';

  async connect() {
    this.closedByUs = false;
    const { access } = await tokens.get();
    if (!access) return;

    this.cleanup();
    this.setStatus('connecting');

    const socket = new WebSocket(`${WS_URL}/v1/ws?token=${encodeURIComponent(access)}`);
    this.socket = socket;

    socket.onopen = () => {
      this.attempt = 0;
      this.lastPong = Date.now();
      this.setStatus('online');
      this.startHeartbeat();
    };

    socket.onmessage = (event) => {
      this.lastPong = Date.now();
      try {
        const parsed = JSON.parse(event.data as string) as ServerEvent;
        this.listeners.forEach((listener) => listener(parsed));
      } catch {
        // A frame we cannot parse is a server-side bug; dropping it is safer
        // than crashing the connection that carries everything else.
      }
    };

    socket.onerror = () => {
      // onclose always follows, so reconnection is handled in one place.
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

    // Jitter prevents every client on a flapping network reconnecting in
    // lockstep and hammering the server the moment it recovers.
    const jitter = Math.random() * 400;

    this.reconnectTimer = setTimeout(() => {
      this.reconnectTimer = null;
      this.connect();
    }, delay + jitter);
  }

  private startHeartbeat() {
    this.stopHeartbeat();
    this.heartbeatTimer = setInterval(() => {
      // A mobile radio can drop a connection without either side seeing a
      // close frame. If nothing has arrived in 70s — more than two server
      // heartbeats — treat the socket as dead rather than believing it.
      if (Date.now() - this.lastPong > 70_000) {
        this.socket?.close();
        return;
      }
      this.send({ type: 'ping' });
    }, 25_000);
  }

  private stopHeartbeat() {
    if (this.heartbeatTimer) clearInterval(this.heartbeatTimer);
    this.heartbeatTimer = null;
  }

  private cleanup() {
    if (this.reconnectTimer) clearTimeout(this.reconnectTimer);
    this.reconnectTimer = null;
    this.stopHeartbeat();
    if (this.socket) {
      this.socket.onclose = null;
      this.socket.close();
      this.socket = null;
    }
  }

  private setStatus(next: SocketStatus) {
    if (this.status === next) return;
    this.status = next;
    this.statusListeners.forEach((listener) => listener(next));
  }

  send(payload: unknown) {
    if (this.socket?.readyState === WebSocket.OPEN) {
      this.socket.send(JSON.stringify(payload));
    }
  }

  /** Typing is ephemeral: dropped silently when offline, never queued. */
  sendTyping(conversationId: string, state: 'typing' | 'recording' | 'stopped') {
    this.send({ type: 'typing', data: { conversation_id: conversationId, state } });
  }

  subscribe(listener: Listener) {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  onStatus(listener: StatusListener) {
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
}

export const socket = new RealtimeSocket();
