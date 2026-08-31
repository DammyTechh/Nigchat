import { create } from 'zustand';

import { encodeBase64 } from '../lib/base64';
import { conversations as conversationsApi, messages as messagesApi } from '../lib/endpoints';
import { socket } from '../lib/socket';
import type { ConversationSummary, Message, ServerEvent } from '../lib/types';

interface ChatState {
  conversations: ConversationSummary[];
  messages: Record<string, Message[]>;
  typing: Record<string, string[]>;
  activeId: string | null;
  loaded: boolean;

  load: () => Promise<void>;
  open: (id: string) => Promise<void>;
  loadOlder: (id: string) => Promise<void>;
  send: (id: string, text: string) => Promise<void>;
  markRead: (id: string) => Promise<void>;
  handleEvent: (event: ServerEvent) => void;
  totalUnread: () => number;
}

/** Sorted by `seq` always — never `created_at`, which drifts between clocks. */
function insertBySeq(existing: Message[], incoming: Message): Message[] {
  const filtered = existing.filter(
    (message) =>
      message.seq !== incoming.seq && message.client_message_id !== incoming.client_message_id,
  );
  filtered.push(incoming);
  return filtered.sort((a, b) => a.seq - b.seq);
}

let optimisticSeq = -1;

export const useChats = create<ChatState>((set, get) => ({
  conversations: [],
  messages: {},
  typing: {},
  activeId: null,
  loaded: false,

  async load() {
    const list = await conversationsApi.list();
    set({ conversations: list, loaded: true });
  },

  async open(id) {
    set({ activeId: id });
    if (!get().messages[id]) {
      const page = await messagesApi.list(id, { limit: 50 });
      set((state) => ({
        messages: {
          ...state.messages,
          [id]: [...page.items].sort((a, b) => a.seq - b.seq),
        },
      }));
    }
    get().markRead(id).catch(() => {});
  },

  async loadOlder(id) {
    const current = get().messages[id] ?? [];
    const oldest = current.find((message) => message.seq > 0);
    if (!oldest) return;

    const page = await messagesApi.list(id, { before_seq: oldest.seq, limit: 50 });
    if (!page.items.length) return;

    set((state) => ({
      messages: {
        ...state.messages,
        [id]: [...page.items, ...current].sort((a, b) => a.seq - b.seq),
      },
    }));
  },

  async send(id, text) {
    // Generated before the request. Retrying with the same value returns the
    // original message instead of creating a duplicate.
    const clientMessageId = crypto.randomUUID();
    const ciphertext = encodeBase64(text);

    const optimistic: Message = {
      id: clientMessageId,
      conversation_id: id,
      seq: optimisticSeq--,
      sender_id: null,
      client_message_id: clientMessageId,
      kind: 'text',
      ciphertext,
      envelope_version: 1,
      system_text: null,
      metadata: { pending: true },
      reply_to_id: null,
      expires_at: null,
      edited_at: null,
      deleted_at: null,
      created_at: new Date().toISOString(),
    };

    set((state) => ({
      messages: { ...state.messages, [id]: [...(state.messages[id] ?? []), optimistic] },
    }));

    try {
      const saved = await messagesApi.send({
        conversation_id: id,
        client_message_id: clientMessageId,
        ciphertext,
      });
      set((state) => ({
        messages: {
          ...state.messages,
          [id]: insertBySeq(
            (state.messages[id] ?? []).filter(
              (message) => message.client_message_id !== clientMessageId,
            ),
            saved,
          ),
        },
      }));
    } catch {
      // Mark failed rather than dropping it. The text is the user's.
      set((state) => ({
        messages: {
          ...state.messages,
          [id]: (state.messages[id] ?? []).map((message) =>
            message.client_message_id === clientMessageId
              ? { ...message, metadata: { failed: true } }
              : message,
          ),
        },
      }));
    }
  },

  async markRead(id) {
    const head = (get().messages[id] ?? []).reduce((max, m) => Math.max(max, m.seq), 0);
    if (head <= 0) return;

    set((state) => ({
      conversations: state.conversations.map((conversation) =>
        conversation.id === id
          ? { ...conversation, unread_count: 0, last_read_seq: head }
          : conversation,
      ),
    }));

    await conversationsApi.markRead(id, head).catch(() => {});
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

        const isActive = get().activeId === conversation_id;

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
                    // An open conversation is being read, so it does not
                    // accumulate an unread count while you look at it.
                    unread_count: isActive ? 0 : conversation.unread_count + 1,
                    last_message_at: event.data.created_at,
                    last_message_kind: event.data.kind,
                  }
                : conversation,
            )
            .sort((a, b) => {
              if (a.is_pinned !== b.is_pinned) return a.is_pinned ? -1 : 1;
              return (b.last_message_at ?? '').localeCompare(a.last_message_at ?? '');
            }),
        }));

        conversationsApi.markDelivered(conversation_id, event.data.seq).catch(() => {});
        if (isActive) get().markRead(conversation_id).catch(() => {});
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
          return {
            typing: {
              ...state.typing,
              [conversation_id]:
                typingState === 'stopped'
                  ? current.filter((id) => id !== user_id)
                  : Array.from(new Set([...current, user_id])),
            },
          };
        });

        // Expire on our own clock. A 'stopped' frame that never arrives would
        // otherwise leave a chat permanently "typing…".
        window.setTimeout(() => {
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
        get().load().catch(() => {});
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

socket.subscribe((event) => useChats.getState().handleEvent(event));
