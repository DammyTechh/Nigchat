import clsx from 'clsx';
import { BellOff, Lock, MessageSquarePlus, Pin, Search, SquarePen, Users, X } from 'lucide-react';
import { useMemo, useState } from 'react';

import { listTimestamp } from '../lib/format';
import type { ConversationSummary } from '../lib/types';
import { useChats } from '../store/chats';
import { Avatar, Badge, EmptyState, IconButton, SkeletonRow } from './primitives';

type Filter = 'all' | 'unread' | 'groups';

const FILTERS: { value: Filter; label: string }[] = [
  { value: 'all', label: 'All' },
  { value: 'unread', label: 'Unread' },
  { value: 'groups', label: 'Groups' },
];

export function ConversationList({
  onSelect,
  activeId,
}: {
  onSelect: (id: string) => void;
  activeId: string | null;
}) {
  const conversations = useChats((state) => state.conversations);
  const loaded = useChats((state) => state.loaded);

  const [query, setQuery] = useState('');
  const [filter, setFilter] = useState<Filter>('all');

  const visible = useMemo(() => {
    const term = query.trim().toLowerCase();
    return conversations.filter((conversation) => {
      if (conversation.is_archived) return false;
      if (filter === 'unread' && conversation.unread_count === 0) return false;
      if (filter === 'groups' && conversation.kind === 'direct') return false;
      if (term && !(conversation.title ?? '').toLowerCase().includes(term)) return false;
      return true;
    });
  }, [conversations, filter, query]);

  return (
    <div className="flex h-full flex-col">
      {/* Sticky glass header: the list scrolls beneath it rather than pushing
          it away, so search and filters stay reachable in a long list. */}
      <div className="glass sticky top-0 z-10 border-b border-line px-4 pb-3 pt-5">
        <div className="flex items-center justify-between">
          <h1 className="text-title font-bold">Chats</h1>
          <IconButton icon={SquarePen} label="New chat" />
        </div>

        <div className="relative mt-4">
          <Search
            size={16}
            className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-ink-3"
          />
          <input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Search chats"
            aria-label="Search chats"
            className={clsx(
              'w-full rounded-xl border border-line bg-raised py-2.5 pl-9 pr-9',
              'text-callout placeholder:text-ink-3',
              'focus:border-brand focus:outline-none',
            )}
          />
          {query && (
            <button
              type="button"
              onClick={() => setQuery('')}
              aria-label="Clear search"
              className="absolute right-2 top-1/2 -translate-y-1/2 rounded-full p-1 text-ink-3 hover:bg-pressed"
            >
              <X size={14} />
            </button>
          )}
        </div>

        <div className="mt-3 flex gap-1 rounded-xl bg-raised p-1">
          {FILTERS.map((option) => (
            <button
              key={option.value}
              type="button"
              onClick={() => setFilter(option.value)}
              aria-pressed={filter === option.value}
              className={clsx(
                'flex-1 rounded-lg py-1.5 text-subhead transition-colors',
                filter === option.value
                  ? 'border border-line bg-surface text-ink'
                  : 'text-ink-3 hover:text-ink-2',
              )}
            >
              {option.label}
            </button>
          ))}
        </div>
      </div>

      <div className="scroll-thin flex-1 overflow-y-auto">
        {!loaded ? (
          Array.from({ length: 8 }).map((_, index) => <SkeletonRow key={index} />)
        ) : visible.length === 0 ? (
          <EmptyState
            icon={MessageSquarePlus}
            title={query ? 'No matches' : 'No conversations yet'}
            message={
              query ? `Nothing found for "${query}".` : 'Start a chat on your phone to see it here.'
            }
          />
        ) : (
          visible.map((conversation) => (
            <ConversationRow
              key={conversation.id}
              conversation={conversation}
              active={conversation.id === activeId}
              onSelect={onSelect}
            />
          ))
        )}
      </div>
    </div>
  );
}

function ConversationRow({
  conversation,
  active,
  onSelect,
}: {
  conversation: ConversationSummary;
  active: boolean;
  onSelect: (id: string) => void;
}) {
  const name = conversation.title ?? 'Unknown';
  const muted = !!conversation.muted_until;
  const unread = conversation.unread_count > 0;

  return (
    <button
      type="button"
      onClick={() => onSelect(conversation.id)}
      aria-current={active ? 'true' : undefined}
      className={clsx(
        'flex w-full items-center gap-3 px-4 py-3 text-left transition-colors',
        active ? 'bg-brand-soft' : 'hover:bg-raised',
      )}
    >
      <Avatar name={name} size={46} />

      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <span className="truncate text-headline">{name}</span>
          <span
            className={clsx(
              'ml-auto shrink-0 text-caption',
              // Unread puts the timestamp in brand green — the eye finds the
              // new conversation without reading a word.
              unread && !muted ? 'font-semibold text-brand' : 'text-ink-3',
            )}
          >
            {listTimestamp(conversation.last_message_at)}
          </span>
        </div>

        <div className="mt-0.5 flex items-center gap-1.5">
          {conversation.kind === 'group' && <Users size={13} className="shrink-0 text-ink-3" />}
          {conversation.is_locked && <Lock size={13} className="shrink-0 text-ink-3" />}
          <span
            className={clsx('truncate text-callout', unread ? 'text-ink-2' : 'text-ink-3')}
          >
            {previewFor(conversation)}
          </span>
          {conversation.is_pinned && <Pin size={13} className="shrink-0 text-ink-3" />}
          {muted && <BellOff size={13} className="shrink-0 text-ink-3" />}
          <Badge count={conversation.unread_count} muted={muted} />
        </div>
      </div>
    </button>
  );
}

function previewFor(conversation: ConversationSummary) {
  if (conversation.is_locked) return 'Locked chat';
  const labels: Record<string, string> = {
    image: 'Photo',
    video: 'Video',
    voice_note: 'Voice message',
    audio: 'Audio',
    document: 'Document',
    sticker: 'Sticker',
    gif: 'GIF',
    location: 'Location',
    contact: 'Contact',
    poll: 'Poll',
    call_event: 'Call',
  };
  const kind = conversation.last_message_kind;
  if (kind && kind !== 'text') return labels[kind] ?? 'Message';
  return 'Open conversation';
}
