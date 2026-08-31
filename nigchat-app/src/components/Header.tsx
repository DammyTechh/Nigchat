import { useRouter } from 'expo-router';
import React from 'react';
import { StyleSheet, View, ViewStyle } from 'react-native';

import { layout, spacing, useColors } from '../theme';
import { IconButton } from './Button';
import { IconName } from './Icon';
import { Text } from './Text';

interface Action {
  icon: IconName;
  onPress: () => void;
  label: string;
}

interface HeaderProps {
  title?: string;
  subtitle?: string;
  /** Renders the title at display size below the bar, iOS large-title style. */
  large?: boolean;
  back?: boolean;
  onBack?: () => void;
  actions?: Action[];
  /** Replaces the title area entirely — used by the chat screen. */
  center?: React.ReactNode;
  style?: ViewStyle;
  borderless?: boolean;
}

/**
 * The app's header.
 *
 * Note what it is *not*: a coloured bar. The header sits on the page
 * background, and the only rule separating it from content is a hairline that
 * appears when the header has actions. Green never appears here. This single
 * decision does more to distance the app from its obvious competitor than any
 * amount of icon restyling.
 */
export function Header({
  title,
  subtitle,
  large,
  back,
  onBack,
  actions = [],
  center,
  style,
  borderless,
}: HeaderProps) {
  const colors = useColors();
  const router = useRouter();

  const handleBack = () => {
    if (onBack) return onBack();
    if (router.canGoBack()) router.back();
  };

  return (
    <View style={style}>
      <View
        style={[
          styles.bar,
          {
            minHeight: layout.headerHeight,
            borderBottomWidth: borderless ? 0 : StyleSheet.hairlineWidth,
            borderBottomColor: colors.border,
          },
        ]}
      >
        <View style={styles.side}>
          {back && (
            <IconButton icon="ChevronLeft" onPress={handleBack} accessibilityLabel="Go back" size={40} />
          )}
        </View>

        <View style={styles.middle}>
          {center ??
            (!large && title ? (
              <View style={{ alignItems: 'center' }}>
                <Text variant="titleSmall" numberOfLines={1}>
                  {title}
                </Text>
                {subtitle ? (
                  <Text variant="caption" tone="muted" numberOfLines={1}>
                    {subtitle}
                  </Text>
                ) : null}
              </View>
            ) : null)}
        </View>

        <View style={[styles.side, styles.actions]}>
          {actions.map((action) => (
            <IconButton
              key={action.label}
              icon={action.icon}
              onPress={action.onPress}
              accessibilityLabel={action.label}
              size={40}
            />
          ))}
        </View>
      </View>

      {large && title ? (
        <View style={styles.largeTitle}>
          <Text variant="displayLarge">{title}</Text>
          {subtitle ? (
            <Text variant="footnote" tone="muted" style={{ marginTop: 2 }}>
              {subtitle}
            </Text>
          ) : null}
        </View>
      ) : null}
    </View>
  );
}

const styles = StyleSheet.create({
  bar: {
    flexDirection: 'row',
    alignItems: 'center',
    paddingHorizontal: spacing.xs,
  },
  side: { minWidth: 48, flexDirection: 'row', alignItems: 'center' },
  actions: { justifyContent: 'flex-end' },
  middle: { flex: 1, alignItems: 'center', justifyContent: 'center' },
  largeTitle: {
    paddingHorizontal: spacing.base,
    paddingTop: spacing.sm,
    paddingBottom: spacing.md,
  },
});
