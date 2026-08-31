/** Mirrors the backend's wire contract (see /docs on the running server). */

export interface TokenPair {
  access_token: string;
  refresh_token: string;
  expires_in: number;
  user_id: string;
  device_id: string;
  is_new_account: boolean;
}

export interface Me {
  id: string;
  phone_e164: string;
  username: string | null;
  display_name: string;
  about: string | null;
  avatar_media_id: string | null;
  two_step_enabled: boolean;
  created_at: string;
}

export interface PublicUser {
  id: string;
  username: string | null;
  display_name: string;
  about: string | null;
  avatar_media_id: string | null;
  last_seen_at: string | null;
}

/** GET /v1/conversations/{id} — the entity, not the list row. */
export interface Conversation {
  id: string;
  kind: 'direct' | 'group' | 'channel';
  title: string | null;
  description: string | null;
  avatar_media_id: string | null;
  only_admins_can_post: boolean;
  disappearing_seconds: number | null;
  created_at: string;
  updated_at: string;
}

/** GET /v1/conversations — denormalised for rendering a list row. */
export interface ConversationSummary {
  id: string;
  kind: 'direct' | 'group' | 'channel';
  title: string | null;
  avatar_media_id: string | null;
  /** Highest seq in the conversation. Compare with last_read_seq. */
  head_seq: number;
  last_read_seq: number;
  unread_count: number;
  last_message_at: string | null;
  last_message_kind: string | null;
  is_pinned: boolean;
  is_archived: boolean;
  is_locked: boolean;
  muted_until: string | null;
  updated_at: string;
}

export interface Message {
  id: string;
  conversation_id: string;
  /** Ordering key. Never sort by created_at. */
  seq: number;
  sender_id: string | null;
  client_message_id: string;
  kind: string;
  /** Base64 ciphertext — decrypt on device. */
  ciphertext: string | null;
  envelope_version: number;
  system_text: string | null;
  metadata: Record<string, unknown>;
  reply_to_id: string | null;
  expires_at: string | null;
  edited_at: string | null;
  deleted_at: string | null;
  created_at: string;
}

export interface Page<T> {
  items: T[];
  has_more: boolean;
  next_cursor: number | null;
}

export interface Device {
  id: string;
  platform: string;
  device_name: string | null;
  app_version: string | null;
  is_primary: boolean;
  linked_at: string;
  last_active_at: string | null;
}

export interface NotificationTone {
  id: string;
  display_name: string;
  category: 'message' | 'group' | 'call' | 'status' | 'system';
  asset_name: string;
  is_default: boolean;
}

export interface QuietHours {
  start_minute: number;
  end_minute: number;
  timezone: string;
  allow_calls: boolean;
}

export interface NotificationPreferences {
  messages_enabled: boolean;
  groups_enabled: boolean;
  calls_enabled: boolean;
  status_enabled: boolean;
  channels_enabled: boolean;
  reactions_enabled: boolean;
  security_alerts_enabled: boolean;
  preview_mode: 'full' | 'name_only' | 'hidden';
  message_tone_id: string | null;
  group_tone_id: string | null;
  call_ringtone_id: string | null;
  vibration: 'off' | 'short' | 'default' | 'long';
  in_app_sounds: boolean;
  quiet_hours: QuietHours | null;
}

export interface SecurityEvent {
  event_type: string;
  severity: 'info' | 'warning' | 'critical';
  device_id: string | null;
  metadata: Record<string, unknown>;
  created_at: string;
}

/** Frames pushed down the WebSocket. */
export type ServerEvent =
  | { type: 'message_created'; data: { conversation_id: string; message_id: string; seq: number; sender_id: string | null; kind: string; ciphertext: string | null; created_at: string } }
  | { type: 'message_edited'; data: { conversation_id: string; seq: number; ciphertext: string | null; edited_at: string } }
  | { type: 'message_deleted'; data: { conversation_id: string; seq: number; for_everyone: boolean } }
  | { type: 'reaction_changed'; data: { conversation_id: string; message_id: string; user_id: string; emoji: string; removed: boolean } }
  | { type: 'read_receipt'; data: { conversation_id: string; user_id: string; last_read_seq: number } }
  | { type: 'delivery_receipt'; data: { conversation_id: string; user_id: string; last_delivered_seq: number } }
  | { type: 'typing'; data: { conversation_id: string; user_id: string; state: 'typing' | 'recording' | 'stopped' } }
  | { type: 'presence'; data: { user_id: string; online: boolean; last_seen_at: string | null } }
  | { type: 'conversation_created'; data: { conversation_id: string; kind: string } }
  | { type: 'membership_changed'; data: { conversation_id: string; user_id: string; change: string } }
  | { type: 'device_event'; data: { device_id: string; event: 'linked' | 'revoked' } }
  | { type: 'key_changed'; data: { user_id: string; device_id: string; key_version: number } }
  | { type: 'sync_required'; data: { conversation_id: string | null; reason: string } }
  | { type: 'heartbeat'; data: { server_event_id: number } };
