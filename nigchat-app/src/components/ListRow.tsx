import React from 'react';
import { StyleSheet, Switch, View, ViewStyle } from 'react-native';

import { radius, spacing, useColors } from '../theme';
import { Icon, IconName } from './Icon';
import { Pressable } from './Pressable';
import { Text } from './Text';

interface ListRowProps {
  title: string;
  subtitle?: string;
  /** Right-aligned secondary text, e.g. the current setting. */
  value?: string;
  icon?: IconName;
  /** Tinted square behind the icon. Used in Settings, not in the chat list. */
  iconTint?: string;
  left?: React.ReactNode;
  right?: React.ReactNode;
  onPress?: () => void;
  chevron?: boolean;
  toggle?: { value: boolean; onChange: (next: boolean) => void };
  danger?: boolean;
  style?: ViewStyle;
}

export function ListRow({
  title,
  subtitle,
  value,
  icon,
  iconTint,
  left,
  right,
  onPress,
  chevron,
  toggle,
  danger,
  style,
}: ListRowProps) {
  const colors = useColors();
  const interactive = !!onPress || !!toggle;

  const body = (
    <View style={[styles.row, style]}>
      {left ??
        (icon ? (
          <View style={[styles.iconTile, { backgroundColor: iconTint ?? colors.surfaceRaised }]}>
            <Icon
              name={icon}
              size={18}
              color={danger ? colors.danger : iconTint ? colors.onPrimary : colors.textSecondary}
            />
          </View>
        ) : null)}

      <View style={styles.text}>
        <Text variant="body" tone={danger ? 'danger' : 'default'} numberOfLines={1}>
          {title}
        </Text>
        {subtitle ? (
          <Text variant="footnote" tone="muted" numberOfLines={2} style={{ marginTop: 1 }}>
            {subtitle}
          </Text>
        ) : null}
      </View>

      <View style={styles.trailing}>
        {value ? (
          <Text variant="subhead" tone="muted" numberOfLines={1}>
            {value}
          </Text>
        ) : null}
        {right}
        {toggle && (
          <Switch
            value={toggle.value}
            onValueChange={toggle.onChange}
            trackColor={{ true: colors.primary, false: colors.borderStrong }}
            thumbColor={colors.background}
            ios_backgroundColor={colors.borderStrong}
          />
        )}
        {chevron && <Icon name="ChevronRight" size={18} color={colors.textMuted} />}
      </View>
    </View>
  );

  if (!interactive) return body;

  return (
    <Pressable onPress={onPress ?? (() => toggle?.onChange(!toggle.value))} accessibilityRole="button">
      {body}
    </Pressable>
  );
}

/**
 * Grouped section, iOS Settings style but with the app's own radius and a
 * hairline instead of a shadow so it holds up in dark mode.
 */
export function Section({
  title,
  footer,
  children,
  style,
}: {
  title?: string;
  footer?: string;
  children: React.ReactNode;
  style?: ViewStyle;
}) {
  const colors = useColors();
  const items = React.Children.toArray(children).filter(Boolean);

  return (
    <View style={[{ marginBottom: spacing.xl }, style]}>
      {title ? (
        <Text variant="overline" tone="muted" style={styles.sectionTitle}>
          {title}
        </Text>
      ) : null}

      <View
        style={[
          styles.card,
          { backgroundColor: colors.surface, borderColor: colors.border },
        ]}
      >
        {items.map((child, index) => (
          <View key={index}>
            {index > 0 && (
              // Inset divider: aligned to the text, not the card edge. Full-bleed
              // dividers make a list look like a spreadsheet.
              <View style={[styles.divider, { backgroundColor: colors.border }]} />
            )}
            {child}
          </View>
        ))}
      </View>

      {footer ? (
        <Text variant="footnote" tone="muted" style={styles.footer}>
          {footer}
        </Text>
      ) : null}
    </View>
  );
}

const styles = StyleSheet.create({
  row: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: spacing.md,
    paddingHorizontal: spacing.base,
    paddingVertical: 13,
    minHeight: 54,
  },
  iconTile: {
    width: 32,
    height: 32,
    borderRadius: radius.sm,
    alignItems: 'center',
    justifyContent: 'center',
  },
  text: { flex: 1 },
  trailing: { flexDirection: 'row', alignItems: 'center', gap: spacing.sm },
  card: {
    borderRadius: radius.lg,
    borderWidth: StyleSheet.hairlineWidth,
    overflow: 'hidden',
  },
  divider: { height: StyleSheet.hairlineWidth, marginLeft: spacing.base },
  sectionTitle: { marginBottom: spacing.sm, marginLeft: spacing.xs },
  footer: { marginTop: spacing.sm, marginHorizontal: spacing.xs },
});
