import * as Haptics from 'expo-haptics';
import React, { useCallback } from 'react';
import {
  Platform,
  Pressable as RNPressable,
  PressableProps as RNPressableProps,
  StyleProp,
  ViewStyle,
} from 'react-native';

import { useColors } from '../theme';

export interface PressableProps extends Omit<RNPressableProps, 'style'> {
  style?: StyleProp<ViewStyle>;
  /** Adds a pressed-state background. Off for custom-styled buttons. */
  highlight?: boolean;
  /** Light impact on press. Reserve for meaningful actions, not every tap. */
  haptic?: boolean;
}

/**
 * One press primitive so feedback is identical everywhere.
 *
 * Android gets a native ripple, iOS gets an opacity and background shift —
 * matching each platform rather than imposing one look on both. `hitSlop`
 * defaults to 8 because icon-only buttons are frequently drawn smaller than
 * they are comfortable to hit.
 */
export function Pressable({
  style,
  highlight = true,
  haptic = false,
  onPress,
  ...rest
}: PressableProps) {
  const colors = useColors();

  const handlePress = useCallback(
    (event: Parameters<NonNullable<RNPressableProps['onPress']>>[0]) => {
      if (haptic && Platform.OS !== 'web') {
        Haptics.impactAsync(Haptics.ImpactFeedbackStyle.Light).catch(() => {});
      }
      onPress?.(event);
    },
    [haptic, onPress],
  );

  return (
    <RNPressable
      hitSlop={8}
      android_ripple={highlight ? { color: colors.surfacePressed, borderless: false } : undefined}
      onPress={handlePress}
      style={({ pressed }) => [
        style,
        pressed && highlight && Platform.OS === 'ios'
          ? { backgroundColor: colors.surfacePressed, opacity: 0.98 }
          : null,
      ]}
      {...rest}
    />
  );
}
