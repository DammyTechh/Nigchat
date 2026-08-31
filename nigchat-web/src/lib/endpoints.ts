import { api } from './api';
import type {
  ConversationSummary,
  Device,
  Me,
  Message,
  NotificationPreferences,
  NotificationTone,
  Page,
} from './types';

export const conversations = {
  list: () => api.get<ConversationSummary[]>('/v1/conversations'),
  markRead: (id: string, last_read_seq: number) =>
    api.post<{ seq: number }>(`/v1/conversations/${id}/read`, { last_read_seq }),
  markDelivered: (id: string, last_delivered_seq: number) =>
    api.post<{ seq: number }>(`/v1/conversations/${id}/delivered`, { last_delivered_seq }),
  mute: (id: string, duration: 'eight_hours' | 'one_week' | 'always' | null) =>
    api.post(`/v1/conversations/${id}/mute`, { duration }),
};

export const messages = {
  list: (id: string, params: { before_seq?: number; limit?: number } = {}) => {
    const query = new URLSearchParams();
    if (params.before_seq !== undefined) query.set('before_seq', String(params.before_seq));
    query.set('limit', String(params.limit ?? 50));
    return api.get<Page<Message>>(`/v1/conversations/${id}/messages?${query}`);
  },
  send: (body: {
    conversation_id: string;
    client_message_id: string;
    ciphertext: string;
    reply_to_id?: string;
  }) => api.post<Message>('/v1/messages', body),
  remove: (messageId: string, forEveryone: boolean) =>
    api.delete<{ seq: number }>(`/v1/messages/${messageId}?for_everyone=${forEveryone}`),
};

export const users = {
  me: () => api.get<Me>('/v1/me'),
};

export const devices = {
  list: () => api.get<Device[]>('/v1/me/devices'),
  revoke: (id: string) => api.delete<{ ok: boolean }>(`/v1/me/devices/${id}`),
};

export const notifications = {
  tones: () => api.get<NotificationTone[]>('/v1/notifications/tones'),
  preferences: () => api.get<NotificationPreferences>('/v1/notifications/preferences'),
  update: (body: Partial<NotificationPreferences>) =>
    api.patch<NotificationPreferences>('/v1/notifications/preferences', body),
};

export const auth = {
  logout: () => api.post<{ ok: boolean }>('/v1/auth/logout'),
};
