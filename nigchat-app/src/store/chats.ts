import { create } from 'zustand';

import { encodeBase64 } from '../utils/base64';

import { conversations as conversationsApi, messages as messagesApi } from '../api/endpoints';
import { socket } from '../api/socket';
import type { ConversationSummary, Message, ServerEvent } from '../api/types';

interface ChatState {
  conversations: ConversationSummary[];
  /** Messages per conversation, oldest first. */
  messages: Record<string, Message[]>;
  /** user ids currently typing, per conversation. */
  typing: Record<string, string[]>;
  loading: boolean;
  loaded: boolean;

  loadConversations: () => Promise<void>;
  loadMessages: (conversationId: string) => Promise<void>;
  loadOlder: (conversationId: string) => Promise<void>;
  send: (conversationId: string, text: string, replyToId?: string) => Promise<void>;
  markRead: (conversationId: string) => Promise<void>;
  handleEvent: (event: ServerEvent) => void;
  totalUnread: () => number;
}

/**
 * Messages are held in memory keyed by conversation and always sorted by `seq`.
 *
 * `seq` is the only ordering key. Sorting by `created_at` would put an
 * optimistic local message in the wrong place the moment a device clock is a
 * few seconds off, which users read as messages "jumping around".
 */
function insertBySeq(existing: Message[], incoming: Message): Message[] {
  const withoutDuplicate = existing.filter(
    (message) =>
      message.seq !== incoming.seq && message.client_message_id !== incoming.client_message_id,
  );
  withoutDuplicate.push(incoming);
  return withoutDuplicate.sort((a, b) => a.seq - b.seq);
}

/** Optimistic messages get a negative seq so they sort after everything real. */
let optimisticSeq = -1;

export const useChats = create<ChatState>((set, get) => ({
  conversations: [],
  messages: {},
  typing: {},
  loading: false,
  loaded: false,

  async loadConversations() {
    set({ loading: true });
    try {
      const list = await conversationsApi.list();
      set({ conversations: list, loaded: true });
    } finally {
      set({ loading: false });
    }
  },

  async loadMessages(conversationId) {
    const page = await messagesApi.list(conversationId, { limit: 50 });
    // The API returns newest first when scrolling back; the UI wants oldest
    // first, so reverse once here rather than in every render.
    const ordered = [...page.items].sort((a, b) => a.seq - b.seq);
    set((state) => ({ messages: { ...state.messages, [conversationId]: ordered } }));
  },

  async loadOlder(conversationId) {
    const current = get().messages[conversationId] ?? [];
    const oldest = current.find((message) => message.seq > 0);
    if (!oldest) return;

    const page = await messagesApi.list(conversationId, {
      before_seq: oldest.seq,
      limit: 50,
    });
    if (page.items.length === 0) return;

    const merged = [...page.items, ...current].sort((a, b) => a.seq - b.seq);
    set((state) => ({ messages: { ...state.messages, [conversationId]: merged } }));
  },

  async send(conversationId, text, replyToId) {
    // Generated on the device before the request. Retrying with the same value
    // returns the original message instead of creating a duplicate — this is
    // what makes a send safe to retry on a dropped connection.
    const clientMessageId = globalThis.crypto?.randomUUID
      ? globalThis.crypto.randomUUID()
      : `${Date.now()}-${Math.random().toString(16).slice(2)}`;

    // TODO(e2ee): this is where the Signal session encrypts `text`. Until the
    // crypto layer lands the transport still carries base64, so the wire format
    // does not change when it does.
    const ciphertext = encodeBase64(text);

    const optimistic: Message = {
      id: clientMessageId,
      conversation_id: conversationId,
      seq: optimisticSeq--,
      sender_id: null,
      client_message_id: clientMessageId,
      kind: 'text',
      ciphertext,
      envelope_version: 1,
      system_text: null,
      metadata: { pending: true },
      reply_to_id: replyToId ?? null,
      expires_at: null,
      edited_at: null,
      deleted_at: null,
      created_at: new Date().toISOString(),
    };

    // Render immediately. A message that waits for the server before appearing
    // makes the whole app feel slow on a 3G connection.
    set((state) => ({
      messages: {
        ...state.messages,
        [conversationId]: [...(state.messages[conversationId] ?? []), optimistic],
      },
    }));

    try {
      const saved = await messagesApi.send({
        conversation_id: conversationId,
        client_message_id: clientMessageId,
        ciphertext,
        reply_to_id: replyToId,
      });

      set((state) => {
        const list = (state.messages[conversationId] ?? []).filter(
          (message) => message.client_message_id !== clientMessageId,
        );
        return {
          messages: { ...state.messages, [conversationId]: insertBySeq(list, saved) },
        };
      });
    } catch {
      // Mark failed rather than removing it: the text is the user's, and
      // silently discarding it is unforgivable.
      set((state) => ({
        messages: {
          ...state.messages,
          [conversationId]: (state.messages[conversationId] ?? []).map((message) =>
            message.client_message_id === clientMessageId
              ? { ...message, metadata: { failed: true } }
              : message,
          ),
        },
      }));
    }
  },

  async markRead(conversationId) {
    const list = get().messages[conversationId] ?? [];
    const head = list.reduce((max, message) => Math.max(max, message.seq), 0);
    if (head <= 0) return;

    set((state) => ({
      conversations: state.conversations.map((conversation) =>
        conversation.id === conversationId
          ? { ...conversation, unread_count: 0, last_read_seq: head }
          : conversation,
      ),
    }));

    await conversationsApi.markRead(conversationId, head).catch(() => {});
  },

  handleEvent(event) {
    switch (event.type) {
      case 'message_created': {
        const { conversation_id } = event.data;
        const message: Message = {
          id: event.data.message_id,
          conversation_id,
          seq: event.data.seq,
          sender_id: event.data.sender_id,
          client_message_id: event.data.message_id,
          kind: event.data.kind,
          ciphertext: event.data.ciphertext,
          envelope_version: 1,
          system_text: null,
          metadata: {},
          reply_to_id: null,
          expires_at: null,
          edited_at: null,
          deleted_at: null,
          created_at: event.data.created_at,
        };

        set((state) => ({
          messages: {
            ...state.messages,
            [conversation_id]: insertBySeq(state.messages[conversation_id] ?? [], message),
          },
          conversations: state.conversations
            .map((conversation) =>
              conversation.id === conversation_id
                ? {
                    ...conversation,
                    head_seq: Math.max(conversation.head_seq, event.data.seq),
                    unread_count: conversation.unread_count + 1,
                    last_message_at: event.data.created_at,
                    last_message_kind: event.data.kind,
                  }
                : conversation,
            )
            // Newest conversation to the top, pinned always above.
            .sort((a, b) => {
              if (a.is_pinned !== b.is_pinned) return a.is_pinned ? -1 : 1;
              return (b.last_message_at ?? '').localeCompare(a.last_message_at ?? '');
            }),
        }));

        // Tell the sender it arrived, so their first tick fills in.
        conversationsApi.markDelivered(conversation_id, event.data.seq).catch(() => {});
        break;
      }

      case 'message_deleted': {
        const { conversation_id, seq } = event.data;
        set((state) => ({
          messages: {
            ...state.messages,
            [conversation_id]: (state.messages[conversation_id] ?? []).map((message) =>
              message.seq === seq
                ? { ...message, deleted_at: new Date().toISOString(), ciphertext: null }
                : message,
            ),
          },
        }));
        break;
      }

      case 'typing': {
        const { conversation_id, user_id, state: typingState } = event.data;
        set((state) => {
          const current = state.typing[conversation_id] ?? [];
          const next =
            typingState === 'stopped'
              ? current.filter((id) => id !== user_id)
              : Array.from(new Set([...current, user_id]));
          return { typing: { ...state.typing, [conversation_id]: next } };
        });

        // Indicators expire on their own. Relying on a 'stopped' frame that may
        // never arrive leaves a chat permanently "typing…".
        setTimeout(() => {
          set((state) => ({
            typing: {
              ...state.typing,
              [conversation_id]: (state.typing[conversation_id] ?? []).filter(
                (id) => id !== user_id,
              ),
            },
          }));
        }, 6_000);
        break;
      }

      case 'sync_required':
      case 'conversation_created':
        get().loadConversations().catch(() => {});
        break;

      default:
        break;
    }
  },

  totalUnread() {
    return get().conversations.reduce(
      (total, conversation) => total + (conversation.muted_until ? 0 : conversation.unread_count),
      0,
    );
  },
}));

// One subscription for the whole app, wired once at module load.
socket.subscribe((event) => useChats.getState().handleEvent(event));
