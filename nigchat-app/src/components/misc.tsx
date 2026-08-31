import React from 'react';
import { StyleSheet, View, ViewStyle } from 'react-native';

import { radius, spacing, useColors } from '../theme';
import { Button } from './Button';
import { Icon, IconName } from './Icon';
import { Pressable } from './Pressable';
import { Text } from './Text';

/** Unread count. Caps at 99+ so a busy group cannot widen the row. */
export function Badge({ count, muted }: { count: number; muted?: boolean }) {
  const colors = useColors();
  if (count <= 0) return null;

  return (
    <View
      style={[
        styles.badge,
        { backgroundColor: muted ? colors.textMuted : colors.primary },
      ]}
    >
      <Text variant="caption" style={{ color: colors.onPrimary }} allowFontScaling={false}>
        {count > 99 ? '99+' : count}
      </Text>
    </View>
  );
}

/** Empty state. Every list has one — a blank screen reads as a bug. */
export function EmptyState({
  icon,
  title,
  message,
  actionLabel,
  onAction,
}: {
  icon: IconName;
  title: string;
  message: string;
  actionLabel?: string;
  onAction?: () => void;
}) {
  const colors = useColors();
  return (
    <View style={styles.empty}>
      <View style={[styles.emptyIcon, { backgroundColor: colors.primarySoft }]}>
        <Icon name={icon} size={26} color={colors.primary} strokeWidth={1.75} />
      </View>
      <Text variant="titleSmall" center style={{ marginTop: spacing.base }}>
        {title}
      </Text>
      <Text variant="subhead" tone="muted" center style={{ marginTop: spacing.xs, maxWidth: 280 }}>
        {message}
      </Text>
      {actionLabel && onAction ? (
        <Button label={actionLabel} onPress={onAction} style={{ marginTop: spacing.lg }} />
      ) : null}
    </View>
  );
}

/** Segmented control for two or three exclusive options. */
export function SegmentedControl<T extends string>({
  options,
  value,
  onChange,
  style,
}: {
  options: { value: T; label: string }[];
  value: T;
  onChange: (next: T) => void;
  style?: ViewStyle;
}) {
  const colors = useColors();

  return (
    <View style={[styles.segment, { backgroundColor: colors.surfaceRaised }, style]}>
      {options.map((option) => {
        const active = option.value === value;
        return (
          <Pressable
            key={option.value}
            onPress={() => onChange(option.value)}
            highlight={false}
            accessibilityRole="tab"
            accessibilityState={{ selected: active }}
            style={[
              styles.segmentItem,
              active && {
                backgroundColor: colors.surface,
                borderColor: colors.border,
                borderWidth: StyleSheet.hairlineWidth,
              },
            ]}
          >
            <Text variant="subhead" tone={active ? 'default' : 'muted'} center>
              {option.label}
            </Text>
          </Pressable>
        );
      })}
    </View>
  );
}

/** Skeleton row for the chat list while the first sync lands. */
export function SkeletonRow() {
  const colors = useColors();
  return (
    <View style={styles.skeletonRow}>
      <View style={[styles.skelCircle, { backgroundColor: colors.surfaceRaised }]} />
      <View style={{ flex: 1, gap: 8 }}>
        <View style={[styles.skelLine, { backgroundColor: colors.surfaceRaised, width: '45%' }]} />
        <View style={[styles.skelLine, { backgroundColor: colors.surfaceRaised, width: '78%' }]} />
      </View>
    </View>
  );
}

/** Inline banner — offline state, key-change warnings, security notices. */
export function Banner({
  tone = 'info',
  icon,
  text,
  onPress,
}: {
  tone?: 'info' | 'warning' | 'danger';
  icon: IconName;
  text: string;
  onPress?: () => void;
}) {
  const colors = useColors();
  const accent =
    tone === 'danger' ? colors.danger : tone === 'warning' ? colors.warning : colors.primary;

  return (
    <Pressable onPress={onPress} highlight={!!onPress}>
      <View style={[styles.banner, { backgroundColor: colors.surfaceRaised, borderLeftColor: accent }]}>
        <Icon name={icon} size={16} color={accent} />
        <Text variant="footnote" style={{ flex: 1 }}>
          {text}
        </Text>
        {onPress && <Icon name="ChevronRight" size={16} color={colors.textMuted} />}
      </View>
    </Pressable>
  );
}

const styles = StyleSheet.create({
  badge: {
    minWidth: 20,
    height: 20,
    borderRadius: radius.pill,
    paddingHorizontal: 6,
    alignItems: 'center',
    justifyContent: 'center',
  },
  empty: {
    flex: 1,
    alignItems: 'center',
    justifyContent: 'center',
    padding: spacing.xl,
  },
  emptyIcon: {
    width: 64,
    height: 64,
    borderRadius: 32,
    alignItems: 'center',
    justifyContent: 'center',
  },
  segment: { flexDirection: 'row', borderRadius: radius.md, padding: 3, gap: 3 },
  segmentItem: {
    flex: 1,
    paddingVertical: 7,
    borderRadius: radius.sm,
    alignItems: 'center',
  },
  skeletonRow: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: spacing.md,
    paddingHorizontal: spacing.base,
    paddingVertical: 14,
  },
  skelCircle: { width: 52, height: 52, borderRadius: 26 },
  skelLine: { height: 11, borderRadius: 6 },
  banner: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: spacing.md,
    padding: spacing.md,
    marginHorizontal: spacing.base,
    marginBottom: spacing.sm,
    borderRadius: radius.md,
    borderLeftWidth: 3,
  },
});
