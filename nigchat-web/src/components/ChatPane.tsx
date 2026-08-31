import clsx from 'clsx';
import {
  ArrowLeft,
  ArrowUp,
  Check,
  CheckCheck,
  CircleAlert,
  Clock3,
  Lock,
  MessageSquare,
  Paperclip,
  Phone,
  Search,
  Smile,
  Video,
} from 'lucide-react';
import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';

import { decodeBase64 } from '../lib/base64';
import { bubbleTime, dayLabel, sameDay, shouldGroup } from '../lib/format';
import { socket } from '../lib/socket';
import type { Message } from '../lib/types';
import { useChats } from '../store/chats';
import { useSession } from '../store/session';
import { Avatar, EmptyState, IconButton } from './primitives';

type DeliveryState = 'pending' | 'sent' | 'delivered' | 'read' | 'failed';

function decode(ciphertext: string | null) {
  if (!ciphertext) return '';
  try {
    return decodeBase64(ciphertext);
  } catch {
    return '';
  }
}

export function ChatPane({
  conversationId,
  onBack,
}: {
  conversationId: string | null;
  onBack?: () => void;
}) {
  const myId = useSession((state) => state.userId);
  const conversation = useChats((state) =>
    state.conversations.find((item) => item.id === conversationId),
  );
  const messages = useChats((state) => (conversationId ? state.messages[conversationId] : undefined));
  const typing = useChats((state) => (conversationId ? state.typing[conversationId] : undefined));
  const { send, loadOlder } = useChats();

  const [draft, setDraft] = useState('');
  const scrollRef = useRef<HTMLDivElement>(null);
  const composerRef = useRef<HTMLTextAreaElement>(null);
  const typingTimer = useRef<number | null>(null);

  const list = useMemo(() => messages ?? [], [messages]);

  // Jump to the newest message when the conversation changes or one arrives.
  // `useLayoutEffect` so it happens before paint — with a plain effect the user
  // sees the list at the old offset for one frame.
  useLayoutEffect(() => {
    const element = scrollRef.current;
    if (!element) return;
    element.scrollTop = element.scrollHeight;
  }, [conversationId, list.length]);

  useEffect(() => {
    composerRef.current?.focus();
    setDraft('');
  }, [conversationId]);

  if (!conversationId || !conversation) {
    return (
      <div className="hidden h-full flex-col items-center justify-center bg-raised/40 md:flex">
        <EmptyState
          icon={MessageSquare}
          title="Pick a conversation"
          message="Choose a chat on the left, or start a new one from your phone."
        />
        <p className="mt-6 flex items-center gap-1.5 text-caption text-ink-3">
          <Lock size={12} />
          Your messages are end-to-end encrypted
        </p>
      </div>
    );
  }

  function onDraftChange(value: string) {
    setDraft(value);
    if (!conversationId) return;

    socket.sendTyping(conversationId, 'typing');
    if (typingTimer.current) window.clearTimeout(typingTimer.current);
    // Debounced stop, so a fast typist does not emit a frame per keystroke.
    typingTimer.current = window.setTimeout(
      () => socket.sendTyping(conversationId, 'stopped'),
      2_500,
    );
  }

  async function submit() {
    const text = draft.trim();
    if (!text || !conversationId) return;
    setDraft('');
    socket.sendTyping(conversationId, 'stopped');
    await send(conversationId, text);
  }

  const rows = buildRows(list);
  const title = conversation.title ?? 'Chat';

  return (
    <div className="flex h-full flex-col">
      <header className="glass flex items-center gap-3 border-b border-line px-4 py-3">
        {onBack && (
          <IconButton icon={ArrowLeft} label="Back to chats" onClick={onBack} className="md:hidden" />
        )}
        <Avatar name={title} size={38} />
        <div className="min-w-0 flex-1">
          <div className="truncate text-headline">{title}</div>
          <div className={clsx('truncate text-caption', typing?.length ? 'text-brand' : 'text-ink-3')}>
            {typing?.length
              ? typing.length === 1
                ? 'typing…'
                : `${typing.length} people typing…`
              : conversation.kind === 'group'
                ? 'Group'
                : 'Click for contact info'}
          </div>
        </div>
        <IconButton icon={Search} label="Search in conversation" />
        <IconButton icon={Phone} label="Voice call" />
        <IconButton icon={Video} label="Video call" />
      </header>

      <div
        ref={scrollRef}
        className="scroll-thin flex-1 overflow-y-auto px-4 py-4"
        onScroll={(event) => {
          // Load older history when the user reaches the top.
          if (event.currentTarget.scrollTop < 80) {
            loadOlder(conversationId).catch(() => {});
          }
        }}
      >
        <div className="mx-auto flex max-w-3xl flex-col">
          <SystemNotice icon>
            Messages are end-to-end encrypted. Only you and the people in this chat can read
            them.
          </SystemNotice>

          {rows.map((row) =>
            row.kind === 'day' ? (
              <SystemNotice key={row.id}>{row.label}</SystemNotice>
            ) : row.message.deleted_at ? (
              <SystemNotice key={`m-${row.message.seq}`}>This message was deleted</SystemNotice>
            ) : (
              <Bubble
                key={`m-${row.message.seq}`}
                message={row.message}
                outgoing={row.message.sender_id === myId || row.message.sender_id === null}
                grouped={row.grouped}
                tail={row.tail}
                peerReadSeq={conversation.last_read_seq}
              />
            ),
          )}
        </div>
      </div>

      <div className="glass border-t border-line px-4 py-3">
        <div className="mx-auto flex max-w-3xl items-end gap-2">
          <IconButton icon={Paperclip} label="Attach a file" />

          <div className="flex flex-1 items-end rounded-3xl border border-line bg-raised px-4 py-1">
            <textarea
              ref={composerRef}
              value={draft}
              onChange={(event) => onDraftChange(event.target.value)}
              onKeyDown={(event) => {
                // Enter sends, Shift+Enter breaks the line — the convention
                // every desktop chat client uses.
                if (event.key === 'Enter' && !event.shiftKey) {
                  event.preventDefault();
                  submit();
                }
              }}
              rows={1}
              placeholder="Write a message"
              aria-label="Message"
              className="max-h-40 flex-1 resize-none bg-transparent py-2.5 text-body placeholder:text-ink-3 focus:outline-none"
              style={{ height: 'auto' }}
              onInput={(event) => {
                // Grow with the content, up to the max-height above.
                const element = event.currentTarget;
                element.style.height = 'auto';
                element.style.height = `${Math.min(element.scrollHeight, 160)}px`;
              }}
            />
            <IconButton icon={Smile} label="Emoji" size={32} />
          </div>

          {/* The send button turns solid green only once there is something to
              send — the affordance appears exactly when it means something. */}
          <IconButton
            icon={ArrowUp}
            label="Send"
            variant={draft.trim() ? 'filled' : 'ghost'}
            size={42}
            onClick={submit}
          />
        </div>
      </div>
    </div>
  );
}

type Row =
  | { kind: 'day'; id: string; label: string }
  | { kind: 'message'; message: Message; grouped: boolean; tail: boolean };

function buildRows(messages: Message[]): Row[] {
  const rows: Row[] = [];

  messages.forEach((message, index) => {
    const previous = messages[index - 1];
    const next = messages[index + 1];

    if (!previous || !sameDay(previous.created_at, message.created_at)) {
      rows.push({ kind: 'day', id: `day-${message.seq}`, label: dayLabel(message.created_at) });
    }

    rows.push({
      kind: 'message',
      message,
      grouped: shouldGroup(previous, message),
      tail: !next || !shouldGroup(message, next),
    });
  });

  return rows;
}

function Bubble({
  message,
  outgoing,
  grouped,
  tail,
  peerReadSeq,
}: {
  message: Message;
  outgoing: boolean;
  grouped: boolean;
  tail: boolean;
  peerReadSeq: number;
}) {
  const state = deliveryState(message, peerReadSeq);

  return (
    <div
      className={clsx(
        'flex max-w-[75%] flex-col',
        outgoing ? 'self-end items-end' : 'self-start items-start',
        grouped ? 'mt-0.5' : 'mt-2',
      )}
    >
      <div
        className={clsx(
          'px-3.5 py-2',
          outgoing ? 'bg-bubble-out text-white' : 'border border-line bg-bubble-in text-ink',
          // The app's geometry, exactly: 20px with the tail corner tightened to
          // 6, and 8 at the join of a run so a burst reads as one block.
          'rounded-bubble',
          outgoing && grouped && 'rounded-tr-lg',
          outgoing && tail && 'rounded-br-bubble-tail',
          !outgoing && grouped && 'rounded-tl-lg',
          !outgoing && tail && 'rounded-bl-bubble-tail',
        )}
      >
        <p className="whitespace-pre-wrap break-words text-body">{decode(message.ciphertext)}</p>

        <div
          className={clsx(
            'mt-1 flex items-center justify-end gap-1 text-caption',
            outgoing ? 'text-white/70' : 'text-ink-3',
          )}
        >
          {message.edited_at && <span>edited</span>}
          <span className="tabular-nums">{bubbleTime(message.created_at)}</span>
          {outgoing && <Ticks state={state} />}
        </div>
      </div>
    </div>
  );
}

function Ticks({ state }: { state: DeliveryState }) {
  if (state === 'failed') return <CircleAlert size={13} className="text-danger" />;
  if (state === 'pending') return <Clock3 size={12} />;
  if (state === 'sent') return <Check size={13} />;
  // Delivered and read share a glyph; only the colour changes, so the
  // difference is glanceable without comparing two near-identical marks.
  return <CheckCheck size={14} className={state === 'read' ? 'text-accent' : undefined} />;
}

function deliveryState(message: Message, peerReadSeq: number): DeliveryState {
  if ((message.metadata as { failed?: boolean })?.failed) return 'failed';
  if ((message.metadata as { pending?: boolean })?.pending || message.seq < 0) return 'pending';
  if (peerReadSeq >= message.seq) return 'read';
  return 'delivered';
}

function SystemNotice({ children, icon }: { children: React.ReactNode; icon?: boolean }) {
  return (
    <div className="my-3 flex justify-center">
      <span className="inline-flex max-w-[85%] items-center gap-1.5 rounded-full bg-raised px-3 py-1 text-caption text-ink-3">
        {icon && <Lock size={11} className="shrink-0" />}
        {children}
      </span>
    </div>
  );
}
