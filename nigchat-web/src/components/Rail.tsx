import clsx from 'clsx';
import {
  CircleDashed,
  LogOut,
  MessageSquare,
  Monitor,
  Moon,
  Phone,
  Settings,
  Sun,
} from 'lucide-react';
import { useState } from 'react';

import { useTheme, type ThemePreference } from '../lib/theme';
import { useChats } from '../store/chats';
import { useSession } from '../store/session';
import { Avatar, Badge } from './primitives';

/**
 * The vertical navigation rail.
 *
 * A rail rather than a top bar or a tab strip. It is the shape desktop apps use
 * when the main content is a two-pane list-and-detail, and it keeps the brand
 * green out of the chrome: the active item is a soft green pill, matching the
 * mobile tab bar, rather than a coloured header.
 */

type Panel = 'chats' | 'updates' | 'calls' | 'settings';

const ITEMS: { id: Panel; icon: typeof MessageSquare; label: string }[] = [
  { id: 'chats', icon: MessageSquare, label: 'Chats' },
  { id: 'updates', icon: CircleDashed, label: 'Updates' },
  { id: 'calls', icon: Phone, label: 'Calls' },
];

export function Rail({
  active,
  onChange,
}: {
  active: Panel;
  onChange: (panel: Panel) => void;
}) {
  const unread = useChats((state) => state.totalUnread());
  const me = useSession((state) => state.me);
  const signOut = useSession((state) => state.signOut);
  const { preference, set } = useTheme();
  const [menuOpen, setMenuOpen] = useState(false);

  const nextTheme: Record<ThemePreference, ThemePreference> = {
    system: 'light',
    light: 'dark',
    dark: 'system',
  };
  const ThemeIcon = preference === 'dark' ? Moon : preference === 'light' ? Sun : Monitor;

  return (
    <nav
      aria-label="Main"
      className="flex h-full w-[68px] shrink-0 flex-col items-center border-r border-line bg-surface py-4"
    >
      <img src="/logo-mark.png" alt="NigChat" className="mb-6 h-8 w-8" />

      <div className="flex flex-1 flex-col gap-1">
        {ITEMS.map((item) => (
          <button
            key={item.id}
            type="button"
            onClick={() => onChange(item.id)}
            aria-label={item.label}
            aria-current={active === item.id ? 'page' : undefined}
            title={item.label}
            className="group relative flex h-12 w-12 items-center justify-center"
          >
            <span
              className={clsx(
                'flex h-9 w-11 items-center justify-center rounded-full transition-colors',
                active === item.id
                  ? 'bg-brand-soft text-brand'
                  : 'text-ink-3 group-hover:bg-pressed group-hover:text-ink-2',
              )}
            >
              <item.icon size={20} strokeWidth={active === item.id ? 2.2 : 1.9} />
            </span>

            {item.id === 'chats' && unread > 0 && (
              <span className="absolute right-0 top-1">
                <Badge count={unread} />
              </span>
            )}
          </button>
        ))}
      </div>

      <div className="flex flex-col items-center gap-1">
        <button
          type="button"
          onClick={() => set(nextTheme[preference])}
          aria-label={`Theme: ${preference}. Click to change.`}
          title={`Theme: ${preference}`}
          className="flex h-11 w-11 items-center justify-center rounded-full text-ink-3 transition-colors hover:bg-pressed hover:text-ink-2"
        >
          <ThemeIcon size={19} strokeWidth={1.9} />
        </button>

        <button
          type="button"
          onClick={() => onChange('settings')}
          aria-label="Settings"
          title="Settings"
          className={clsx(
            'flex h-11 w-11 items-center justify-center rounded-full transition-colors',
            active === 'settings'
              ? 'bg-brand-soft text-brand'
              : 'text-ink-3 hover:bg-pressed hover:text-ink-2',
          )}
        >
          <Settings size={19} strokeWidth={1.9} />
        </button>

        <div className="relative">
          <button
            type="button"
            onClick={() => setMenuOpen((open) => !open)}
            aria-label="Account"
            aria-expanded={menuOpen}
            className="mt-1 rounded-full ring-offset-2 ring-offset-surface focus-visible:ring-2"
          >
            <Avatar name={me?.display_name ?? '?'} size={34} />
          </button>

          {menuOpen && (
            <>
              {/* Click-away layer. Without it the menu can only be closed by
                  the same button, which desktop users never expect. */}
              <button
                type="button"
                aria-hidden
                tabIndex={-1}
                className="fixed inset-0 z-40 cursor-default"
                onClick={() => setMenuOpen(false)}
              />
              <div className="glass-strong absolute bottom-0 left-full z-50 ml-2 w-56 rounded-xl border border-line p-1 shadow-raised animate-fade-up">
                <div className="px-3 py-2">
                  <p className="truncate text-callout font-semibold">
                    {me?.display_name ?? 'Signed in'}
                  </p>
                  <p className="truncate text-caption text-ink-3">{me?.phone_e164 ?? ''}</p>
                </div>
                <div className="my-1 h-px bg-line" />
                <button
                  type="button"
                  onClick={signOut}
                  className="flex w-full items-center gap-2 rounded-lg px-3 py-2 text-callout text-danger hover:bg-pressed"
                >
                  <LogOut size={16} />
                  Sign out of this browser
                </button>
              </div>
            </>
          )}
        </div>
      </div>
    </nav>
  );
}

export type { Panel };
