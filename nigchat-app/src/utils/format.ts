import { format, isSameDay, isSameWeek, isSameYear, isToday, isYesterday } from 'date-fns';

/**
 * Timestamp for a chat-list row.
 *
 * Progressive precision: today shows a clock, this week shows a weekday,
 * anything older shows a date. Showing "14:32" next to a message from March is
 * useless, and showing a full date for something five minutes old is noise.
 */
export function listTimestamp(iso: string | null): string {
  if (!iso) return '';
  const date = new Date(iso);
  const now = new Date();

  if (isToday(date)) return format(date, 'HH:mm');
  if (isYesterday(date)) return 'Yesterday';
  if (isSameWeek(date, now, { weekStartsOn: 1 })) return format(date, 'EEE');
  if (isSameYear(date, now)) return format(date, 'd MMM');
  return format(date, 'dd/MM/yy');
}

/** Time inside a bubble. Always exact — this is the message's own clock. */
export function bubbleTime(iso: string): string {
  return format(new Date(iso), 'HH:mm');
}

/** Separator between days in a conversation. */
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

/** "last seen" line under a contact's name. */
export function presenceLabel(online: boolean, lastSeen: string | null): string {
  if (online) return 'online';
  if (!lastSeen) return '';
  const date = new Date(lastSeen);
  if (isToday(date)) return `last seen today at ${format(date, 'HH:mm')}`;
  if (isYesterday(date)) return `last seen yesterday at ${format(date, 'HH:mm')}`;
  return `last seen ${format(date, 'd MMM')}`;
}

/** 1320 -> "22:00". Quiet hours are stored as minutes past local midnight. */
export function minutesToClock(minutes: number): string {
  const hours = Math.floor(minutes / 60);
  const mins = minutes % 60;
  return `${String(hours).padStart(2, '0')}:${String(mins).padStart(2, '0')}`;
}

export function clockToMinutes(clock: string): number {
  const [hours, mins] = clock.split(':').map(Number);
  return (hours || 0) * 60 + (mins || 0);
}

/**
 * Groups a message with the one before it when the same person sent both within
 * a minute. Grouped messages tighten their corners and drop the repeated name,
 * which turns a burst into one visual block.
 */
export function shouldGroup(
  previous: { sender_id: string | null; created_at: string } | undefined,
  current: { sender_id: string | null; created_at: string },
): boolean {
  if (!previous) return false;
  if (previous.sender_id !== current.sender_id) return false;
  const gap = new Date(current.created_at).getTime() - new Date(previous.created_at).getTime();
  return gap < 60_000;
}
