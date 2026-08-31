import { api } from './client';
import type {
  Conversation,
  ConversationSummary,
  Device,
  Me,
  Message,
  NotificationPreferences,
  NotificationTone,
  Page,
  PublicUser,
  SecurityEvent,
  TokenPair,
} from './types';

export const auth = {
  requestOtp: (phone_e164: string) =>
    api.post<{ challenge_sent: boolean; expires_in: number; debug_code?: string }>(
      '/v1/auth/request-otp',
      { phone_e164 },
      { anonymous: true },
    ),

  verifyOtp: (body: {
    phone_e164: string;
    code: string;
    display_name?: string;
    platform: string;
    device_name?: string;
    app_version?: string;
    device_id?: string;
  }) => api.post<TokenPair>('/v1/auth/verify-otp', body, { anonymous: true }),

  logout: () => api.post<{ ok: boolean }>('/v1/auth/logout'),
};

export const users = {
  me: () => api.get<Me>('/v1/me'),
  updateMe: (body: Partial<Pick<Me, 'display_name' | 'about' | 'username'>>) =>
    api.patch<Me>('/v1/me', body),
  get: (id: string) => api.get<PublicUser>(`/v1/users/${id}`),
  byUsername: (username: string) => api.get<PublicUser>(`/v1/users/by-username/${username}`),
  /** Hashes are computed on device; raw numbers never leave the phone. */
  syncContacts: (phone_hashes: string[]) =>
    api.post<PublicUser[]>('/v1/users/sync-contacts', { phone_hashes }),
  block: (user_id: string) => api.post<{ ok: boolean }>('/v1/me/blocks', { user_id }),
  unblock: (userId: string) => api.delete<{ ok: boolean }>(`/v1/me/blocks/${userId}`),
  securityEvents: () => api.get<SecurityEvent[]>('/v1/me/security-events'),

  /** Two-step verification (spec §14). Changing an existing PIN requires it. */
  setTwoStepPin: (pin: string, current_pin?: string) =>
    api.post<{ ok: boolean }>('/v1/me/two-step', { pin, current_pin }),
  disableTwoStep: (pin: string) => api.delete<{ ok: boolean }>('/v1/me/two-step', { pin }),
  verifyTwoStepPin: (pin: string) =>
    api.post<{ ok: boolean }>('/v1/me/two-step/verify', { pin }),
};

export const devices = {
  list: () => api.get<Device[]>('/v1/me/devices'),
  revoke: (deviceId: string) => api.delete<{ ok: boolean }>(`/v1/me/devices/${deviceId}`),
  registerPushToken: (body: {
    provider: 'fcm' | 'apns' | 'web_push';
    token: string;
    is_voip?: boolean;
    sandbox?: boolean;
  }) => api.post<{ ok: boolean }>('/v1/me/devices/push-token', body),
};

export const conversations = {
  list: () => api.get<ConversationSummary[]>('/v1/conversations'),
  get: (id: string) => api.get<Conversation>(`/v1/conversations/${id}`),
  openDirect: (peer_user_id: string) =>
    api.post<{ id: string; kind: string }>('/v1/conversations/direct', { peer_user_id }),
  createGroup: (body: { title: string; description?: string; member_ids: string[] }) =>
    api.post<{ id: string }>('/v1/conversations/group', body),
  addMembers: (id: string, member_ids: string[]) =>
    api.post<string[]>(`/v1/conversations/${id}/members`, { member_ids }),
  leave: (id: string, userId: string) =>
    api.delete<{ ok: boolean }>(`/v1/conversations/${id}/members/${userId}`),
  mute: (id: string, duration: 'eight_hours' | 'one_week' | 'always' | null) =>
    api.post(`/v1/conversations/${id}/mute`, { duration }),
  markRead: (id: string, last_read_seq: number) =>
    api.post<{ seq: number }>(`/v1/conversations/${id}/read`, { last_read_seq }),
  markDelivered: (id: string, last_delivered_seq: number) =>
    api.post<{ seq: number }>(`/v1/conversations/${id}/delivered`, { last_delivered_seq }),
};

export const messages = {
  /** `after_seq` catches up after being offline; `before_seq` scrolls back. */
  list: (conversationId: string, params: { before_seq?: number; after_seq?: number; limit?: number } = {}) => {
    const query = new URLSearchParams();
    if (params.before_seq !== undefined) query.set('before_seq', String(params.before_seq));
    if (params.after_seq !== undefined) query.set('after_seq', String(params.after_seq));
    if (params.limit !== undefined) query.set('limit', String(params.limit));
    const suffix = query.toString() ? `?${query}` : '';
    return api.get<Page<Message>>(`/v1/conversations/${conversationId}/messages${suffix}`);
  },

  send: (body: {
    conversation_id: string;
    client_message_id: string;
    kind?: string;
    ciphertext: string;
    reply_to_id?: string;
    mentions?: string[];
    metadata?: Record<string, unknown>;
  }) => api.post<Message>('/v1/messages', body),

  edit: (messageId: string, ciphertext: string) =>
    api.patch<Message>(`/v1/messages/${messageId}`, { ciphertext }),

  remove: (messageId: string, forEveryone: boolean) =>
    api.delete<{ seq: number }>(`/v1/messages/${messageId}?for_everyone=${forEveryone}`),

  react: (messageId: string, emoji: string, removed = false) =>
    api.post<{ ok: boolean }>(`/v1/messages/${messageId}/reactions`, { emoji, removed }),
};

export const notifications = {
  tones: () => api.get<NotificationTone[]>('/v1/notifications/tones'),
  preferences: () => api.get<NotificationPreferences>('/v1/notifications/preferences'),
  updatePreferences: (body: Partial<NotificationPreferences>) =>
    api.patch<NotificationPreferences>('/v1/notifications/preferences', body),
  conversationSettings: (id: string) =>
    api.get<{
      muted_until: string | null;
      notify_on_mention: boolean;
      tone_id: string | null;
      call_ringtone_id: string | null;
      vibration: string | null;
      preview_mode: string | null;
    }>(`/v1/conversations/${id}/notifications`),
  updateConversationSettings: (
    id: string,
    body: { notify_on_mention?: boolean; tone_id?: string; vibration?: string; preview_mode?: string },
  ) => api.patch(`/v1/conversations/${id}/notifications`, body),
};

export const keys = {
  publish: (body: {
    registration_id: number;
    identity_public_key: string;
    signed_prekey_id: number;
    signed_prekey_public: string;
    signed_prekey_signature: string;
    one_time_prekeys: { key_id: number; public_key: string }[];
  }) => api.post<number>('/v1/keys', body),
  bundles: (userId: string) => api.get(`/v1/keys/${userId}`),
  count: () => api.get<{ remaining: number; needs_top_up: boolean }>('/v1/keys/count'),
};
