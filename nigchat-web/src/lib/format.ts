import { format, isSameDay, isSameWeek, isSameYear, isToday, isYesterday } from 'date-fns';

/** Progressive precision, matching the mobile client exactly. */
export function listTimestamp(iso: string | null): string {
  if (!iso) return '';
  const date = new Date(iso);
  if (isToday(date)) return format(date, 'HH:mm');
  if (isYesterday(date)) return 'Yesterday';
  if (isSameWeek(date, new Date(), { weekStartsOn: 1 })) return format(date, 'EEE');
  if (isSameYear(date, new Date())) return format(date, 'd MMM');
  return format(date, 'dd/MM/yy');
}

export function bubbleTime(iso: string): string {
  return format(new Date(iso), 'HH:mm');
}

export function dayLabel(iso: string): string {
  const date = new Date(iso);
  if (isToday(date)) return 'Today';
  if (isYesterday(date)) return 'Yesterday';
  if (isSameYear(date, new Date())) return format(date, 'EEEE, d MMMM');
  return format(date, 'd MMMM yyyy');
}

export function sameDay(a: string, b: string) {
  return isSameDay(new Date(a), new Date(b));
}

export function shouldGroup(
  previous: { sender_id: string | null; created_at: string } | undefined,
  current: { sender_id: string | null; created_at: string },
): boolean {
  if (!previous) return false;
  if (previous.sender_id !== current.sender_id) return false;
  return new Date(current.created_at).getTime() - new Date(previous.created_at).getTime() < 60_000;
}

/** Deterministic avatar tile, same palette and hash as the mobile app so a
 *  contact keeps the same colour across devices. */
const TILES = ['#0E7A46', '#2F6E52', '#4A6B5C', '#3E7A63', '#1F5E43', '#557066'];

export function avatarTile(name: string) {
  let hash = 0;
  for (let i = 0; i < name.length; i += 1) hash = (hash * 31 + name.charCodeAt(i)) >>> 0;
  return TILES[hash % TILES.length];
}

export function initials(name: string) {
  const words = name.trim().split(/\s+/).filter(Boolean);
  if (!words.length) return '?';
  if (words.length === 1) return words[0].slice(0, 2).toUpperCase();
  return (words[0][0] + words[words.length - 1][0]).toUpperCase();
}
