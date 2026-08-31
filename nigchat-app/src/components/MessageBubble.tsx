import React from 'react';
import { StyleSheet, View } from 'react-native';

import { radius, spacing, useColors } from '../theme';
import { Icon } from './Icon';
import { Text } from './Text';

export type DeliveryState = 'pending' | 'sent' | 'delivered' | 'read' | 'failed';

interface MessageBubbleProps {
  body: string;
  time: string;
  outgoing: boolean;
  state?: DeliveryState;
  /** Sender name, shown once per run in group chats. */
  authorName?: string;
  /** True when the previous message is from the same person within a minute. */
  grouped?: boolean;
  /** Last in a run — gets the tail corner. */
  tail?: boolean;
  edited?: boolean;
  replyTo?: { author: string; preview: string };
}

/**
 * The bubble.
 *
 * Shape is where a messaging app is most recognisable, so this one is
 * deliberately its own: a 20pt radius with the tail corner tightened to 6,
 * incoming bubbles filled and hairlined rather than white-on-grey with a
 * shadow, and no drawn tail triangle at all. Consecutive messages from one
 * person tighten to 8 at the join, which turns a run into a single visual
 * block instead of a stack of identical lozenges.
 */
export function MessageBubble({
  body,
  time,
  outgoing,
  state,
  authorName,
  grouped,
  tail = true,
  edited,
  replyTo,
}: MessageBubbleProps) {
  const colors = useColors();

  const background = outgoing ? colors.bubbleOut : colors.bubbleIn;
  const textColor = outgoing ? colors.bubbleOutText : colors.bubbleInText;
  const metaColor = outgoing ? colors.bubbleOutMeta : colors.bubbleInMeta;

  const corners = {
    borderTopLeftRadius: !outgoing && grouped ? 8 : radius.xl,
    borderTopRightRadius: outgoing && grouped ? 8 : radius.xl,
    borderBottomLeftRadius: !outgoing && tail ? 6 : radius.xl,
    borderBottomRightRadius: outgoing && tail ? 6 : radius.xl,
  };

  return (
    <View
      style={[
        styles.wrapper,
        {
          alignSelf: outgoing ? 'flex-end' : 'flex-start',
          marginTop: grouped ? 2 : spacing.sm,
        },
      ]}
    >
      <View
        style={[
          styles.bubble,
          corners,
          {
            backgroundColor: background,
            borderWidth: outgoing ? 0 : StyleSheet.hairlineWidth,
            borderColor: colors.bubbleInBorder,
          },
        ]}
      >
        {authorName && !outgoing && !grouped ? (
          <Text variant="caption" style={{ color: colors.primary, marginBottom: 3 }}>
            {authorName}
          </Text>
        ) : null}

        {replyTo ? (
          <View
            style={[
              styles.quote,
              {
                // A left rule rather than a filled card: lighter, and it does
                // not turn every reply into a nested box.
                borderLeftColor: outgoing ? colors.bubbleOutMeta : colors.primary,
                backgroundColor: outgoing ? 'rgba(255,255,255,0.12)' : colors.surfacePressed,
              },
            ]}
          >
            <Text variant="caption" style={{ color: metaColor }} numberOfLines={1}>
              {replyTo.author}
            </Text>
            <Text variant="footnote" style={{ color: metaColor }} numberOfLines={1}>
              {replyTo.preview}
            </Text>
          </View>
        ) : null}

        <Text variant="body" style={{ color: textColor }}>
          {body}
        </Text>

        {/* Metadata sits on its own line rather than reserving trailing space
            inside the text run — that trick breaks badly with long words and
            right-to-left scripts. */}
        <View style={styles.meta}>
          {edited ? (
            <Text variant="caption" style={{ color: metaColor }}>
              edited
            </Text>
          ) : null}
          <Text variant="caption" style={{ color: metaColor }}>
            {time}
          </Text>
          {outgoing && state ? <DeliveryTicks state={state} color={metaColor} /> : null}
        </View>
      </View>
    </View>
  );
}

function DeliveryTicks({ state, color }: { state: DeliveryState; color: string }) {
  const colors = useColors();

  if (state === 'failed') {
    return <Icon name="CircleAlert" size={13} color={colors.danger} />;
  }
  if (state === 'pending') {
    return <Icon name="Clock3" size={12} color={color} />;
  }
  if (state === 'sent') {
    return <Icon name="Check" size={13} color={color} />;
  }
  // Delivered and read use the same glyph; only the colour changes, so the
  // difference is glanceable without reading two overlapping ticks.
  return <Icon name="CheckCheck" size={14} color={state === 'read' ? colors.accent : color} />;
}

/** Centred date separator and system notices. */
export function SystemNotice({ text, icon }: { text: string; icon?: 'lock' | 'none' }) {
  const colors = useColors();
  return (
    <View style={styles.noticeRow}>
      <View style={[styles.notice, { backgroundColor: colors.surfaceRaised }]}>
        {icon === 'lock' && <Icon name="Lock" size={11} color={colors.textMuted} />}
        <Text variant="caption" tone="muted" center>
          {text}
        </Text>
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  wrapper: { maxWidth: '82%', paddingHorizontal: spacing.md },
  bubble: { paddingHorizontal: 13, paddingVertical: 9 },
  meta: {
    flexDirection: 'row',
    alignItems: 'center',
    alignSelf: 'flex-end',
    gap: 4,
    marginTop: 3,
  },
  quote: {
    borderLeftWidth: 2.5,
    borderRadius: 6,
    paddingLeft: spacing.sm,
    paddingRight: spacing.sm,
    paddingVertical: 4,
    marginBottom: 5,
  },
  noticeRow: { alignItems: 'center', marginVertical: spacing.base },
  notice: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 5,
    paddingHorizontal: spacing.md,
    paddingVertical: 5,
    borderRadius: radius.pill,
    maxWidth: '86%',
  },
});
