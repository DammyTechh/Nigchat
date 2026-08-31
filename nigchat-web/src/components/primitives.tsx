import clsx from 'clsx';
import type { LucideIcon } from 'lucide-react';
import React from 'react';

import { avatarTile, initials } from '../lib/format';

/**
 * Shared primitives. Every colour comes from a token class, never a hex
 * literal — that is what keeps dark mode correct without auditing each file.
 */

export function Avatar({
  name,
  size = 44,
  ring,
}: {
  name: string;
  size?: number;
  ring?: 'unseen' | 'seen';
}) {
  const tile = avatarTile(name);
  const body = (
    <div
      className="flex shrink-0 items-center justify-center rounded-full font-semibold text-white select-none"
      style={{ width: size, height: size, background: tile, fontSize: size * 0.36 }}
      aria-hidden
    >
      {initials(name)}
    </div>
  );

  if (!ring) return body;

  return (
    <div
      className={clsx(
        'rounded-full p-[3px] ring-2',
        ring === 'unseen' ? 'ring-brand' : 'ring-line',
      )}
    >
      {body}
    </div>
  );
}

export function Button({
  children,
  onClick,
  variant = 'primary',
  icon: Icon,
  disabled,
  loading,
  full,
  className,
  type = 'button',
}: {
  children: React.ReactNode;
  onClick?: () => void;
  variant?: 'primary' | 'secondary' | 'ghost' | 'danger';
  icon?: LucideIcon;
  disabled?: boolean;
  loading?: boolean;
  full?: boolean;
  className?: string;
  type?: 'button' | 'submit';
}) {
  const styles = {
    primary: 'bg-brand text-white hover:bg-brand-deep',
    secondary: 'bg-raised text-ink border border-line hover:bg-pressed',
    ghost: 'text-brand hover:bg-brand-soft',
    danger: 'bg-danger text-white hover:opacity-90',
  }[variant];

  return (
    <button
      type={type}
      onClick={onClick}
      disabled={disabled || loading}
      className={clsx(
        // A pill at every size, matching the app. Corner radius is one of the
        // few places this product is unmistakably itself.
        'inline-flex items-center justify-center gap-2 rounded-full px-5 py-2.5',
        'text-callout font-semibold transition-colors',
        'disabled:cursor-not-allowed disabled:opacity-50',
        full && 'w-full',
        styles,
        className,
      )}
    >
      {loading ? (
        <span className="h-4 w-4 animate-spin rounded-full border-2 border-current border-t-transparent" />
      ) : (
        Icon && <Icon size={17} strokeWidth={2} />
      )}
      {children}
    </button>
  );
}

export function IconButton({
  icon: Icon,
  onClick,
  label,
  variant = 'ghost',
  size = 38,
  className,
}: {
  icon: LucideIcon;
  onClick?: () => void;
  label: string;
  variant?: 'ghost' | 'filled' | 'soft';
  size?: number;
  className?: string;
}) {
  const styles = {
    ghost: 'text-ink-2 hover:bg-pressed',
    filled: 'bg-brand text-white hover:bg-brand-deep',
    soft: 'bg-brand-soft text-brand hover:bg-brand-soft/70',
  }[variant];

  return (
    <button
      type="button"
      onClick={onClick}
      aria-label={label}
      title={label}
      className={clsx(
        'inline-flex shrink-0 items-center justify-center rounded-full transition-colors',
        styles,
        className,
      )}
      style={{ width: size, height: size }}
    >
      <Icon size={size * 0.46} strokeWidth={2} />
    </button>
  );
}

export function Badge({ count, muted }: { count: number; muted?: boolean }) {
  if (count <= 0) return null;
  return (
    <span
      className={clsx(
        'inline-flex h-5 min-w-[20px] items-center justify-center rounded-full px-1.5',
        'text-caption font-medium text-white',
        muted ? 'bg-ink-3' : 'bg-brand',
      )}
    >
      {count > 99 ? '99+' : count}
    </span>
  );
}

export function EmptyState({
  icon: Icon,
  title,
  message,
  action,
}: {
  icon: LucideIcon;
  title: string;
  message: string;
  action?: React.ReactNode;
}) {
  return (
    <div className="flex h-full flex-col items-center justify-center px-8 text-center">
      <div className="flex h-16 w-16 items-center justify-center rounded-full bg-brand-soft">
        <Icon size={26} className="text-brand" strokeWidth={1.75} />
      </div>
      <h2 className="mt-4 text-headline">{title}</h2>
      <p className="mt-1 max-w-sm text-subhead text-ink-3">{message}</p>
      {action && <div className="mt-5">{action}</div>}
    </div>
  );
}

export function Spinner({ className }: { className?: string }) {
  return (
    <span
      className={clsx(
        'inline-block h-5 w-5 animate-spin rounded-full border-2 border-line border-t-brand',
        className,
      )}
      role="status"
      aria-label="Loading"
    />
  );
}

export function SkeletonRow() {
  return (
    <div className="flex items-center gap-3 px-4 py-3">
      <div className="h-11 w-11 shrink-0 animate-pulse rounded-full bg-raised" />
      <div className="flex-1 space-y-2">
        <div className="h-3 w-2/5 animate-pulse rounded bg-raised" />
        <div className="h-3 w-4/5 animate-pulse rounded bg-raised" />
      </div>
    </div>
  );
}
