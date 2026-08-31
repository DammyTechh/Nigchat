import React from 'react';
import { ActivityIndicator, StyleSheet, View, ViewStyle } from 'react-native';

import { layout, radius, spacing, useColors } from '../theme';
import { Icon, IconName } from './Icon';
import { Pressable } from './Pressable';
import { Text } from './Text';

type Variant = 'primary' | 'secondary' | 'ghost' | 'danger';
type Size = 'sm' | 'md' | 'lg';

interface ButtonProps {
  label: string;
  onPress?: () => void;
  variant?: Variant;
  size?: Size;
  icon?: IconName;
  loading?: boolean;
  disabled?: boolean;
  fullWidth?: boolean;
  style?: ViewStyle;
}

const HEIGHTS: Record<Size, number> = { sm: 36, md: 46, lg: 54 };

export function Button({
  label,
  onPress,
  variant = 'primary',
  size = 'md',
  icon,
  loading,
  disabled,
  fullWidth,
  style,
}: ButtonProps) {
  const colors = useColors();
  const inactive = disabled || loading;

  const surface: Record<Variant, ViewStyle> = {
    primary: { backgroundColor: colors.primary },
    secondary: { backgroundColor: colors.surfaceRaised, borderWidth: 1, borderColor: colors.border },
    ghost: { backgroundColor: 'transparent' },
    danger: { backgroundColor: colors.danger },
  };

  const contentColor =
    variant === 'primary' || variant === 'danger'
      ? colors.onPrimary
      : variant === 'ghost'
        ? colors.primary
        : colors.text;

  return (
    <Pressable
      onPress={inactive ? undefined : onPress}
      highlight={false}
      haptic={variant === 'primary'}
      accessibilityRole="button"
      accessibilityState={{ disabled: !!inactive, busy: !!loading }}
      accessibilityLabel={label}
      style={[
        styles.base,
        surface[variant],
        {
          height: HEIGHTS[size],
          // A pill at every size. Corner radius is one of the few places this
          // app is unmistakably itself rather than the platform default.
          borderRadius: radius.pill,
          paddingHorizontal: size === 'sm' ? spacing.base : spacing.xl,
          opacity: inactive ? 0.5 : 1,
          alignSelf: fullWidth ? 'stretch' : 'flex-start',
          minWidth: layout.tapTarget,
        },
        style,
      ]}
    >
      {loading ? (
        <ActivityIndicator color={contentColor} size="small" />
      ) : (
        <View style={styles.content}>
          {icon && <Icon name={icon} size={size === 'sm' ? 16 : 18} color={contentColor} />}
          <Text
            variant={size === 'sm' ? 'subhead' : 'bodyStrong'}
            style={{ color: contentColor }}
            numberOfLines={1}
          >
            {label}
          </Text>
        </View>
      )}
    </Pressable>
  );
}

/** Circular icon-only button — composer send, call actions, FAB. */
export function IconButton({
  icon,
  onPress,
  size = 44,
  variant = 'ghost',
  color,
  accessibilityLabel,
  style,
}: {
  icon: IconName;
  onPress?: () => void;
  size?: number;
  variant?: 'ghost' | 'filled' | 'soft' | 'danger';
  color?: string;
  accessibilityLabel: string;
  style?: ViewStyle;
}) {
  const colors = useColors();

  const background =
    variant === 'filled'
      ? colors.primary
      : variant === 'soft'
        ? colors.primarySoft
        : variant === 'danger'
          ? colors.danger
          : 'transparent';

  const tint =
    color ??
    (variant === 'filled' || variant === 'danger'
      ? colors.onPrimary
      : variant === 'soft'
        ? colors.primary
        : colors.textSecondary);

  return (
    <Pressable
      onPress={onPress}
      highlight={variant === 'ghost'}
      accessibilityRole="button"
      accessibilityLabel={accessibilityLabel}
      style={[
        {
          width: size,
          height: size,
          borderRadius: size / 2,
          backgroundColor: background,
          alignItems: 'center',
          justifyContent: 'center',
        },
        style,
      ]}
    >
      <Icon name={icon} size={size * 0.45} color={tint} />
    </Pressable>
  );
}

const styles = StyleSheet.create({
  base: { alignItems: 'center', justifyContent: 'center' },
  content: { flexDirection: 'row', alignItems: 'center', gap: spacing.sm },
});
