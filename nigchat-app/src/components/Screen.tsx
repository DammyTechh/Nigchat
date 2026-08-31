import { StatusBar } from 'expo-status-bar';
import React from 'react';
import { ScrollView, StyleSheet, useWindowDimensions, View, ViewStyle } from 'react-native';
import { Edge, SafeAreaView } from 'react-native-safe-area-context';

import { layout, useColors, useTheme } from '../theme';

interface ScreenProps {
  children: React.ReactNode;
  /** Wraps content in a ScrollView. Off for list screens that scroll themselves. */
  scroll?: boolean;
  /** Which edges get safe-area padding. Tab screens omit 'bottom'. */
  edges?: Edge[];
  padded?: boolean;
  style?: ViewStyle;
  contentStyle?: ViewStyle;
}

/**
 * Screen shell: safe area, status-bar style, and the responsive width clamp.
 *
 * The clamp is what makes this work on an iPad or a foldable without a separate
 * tablet build. Content stops at 720pt and centres; a chat list stretched
 * across a 12" display is unreadable, and stretching is what most phone-first
 * apps do when they meet a large screen.
 *
 * Safe-area handling covers every iPhone from the SE (no inset) to the Pro Max
 * (Dynamic Island). `SafeAreaView` from react-native-safe-area-context reads the
 * real insets rather than hardcoding notch heights, so a new device shape needs
 * no change here.
 */
export function Screen({
  children,
  scroll,
  edges = ['top'],
  padded,
  style,
  contentStyle,
}: ScreenProps) {
  const colors = useColors();
  const { isDark } = useTheme();
  const { width } = useWindowDimensions();

  const horizontalInset = Math.max(0, (width - layout.maxContentWidth) / 2);

  const inner: ViewStyle = {
    flex: 1,
    paddingHorizontal: (padded ? 16 : 0) + horizontalInset,
  };

  return (
    <SafeAreaView edges={edges} style={[styles.root, { backgroundColor: colors.background }, style]}>
      <StatusBar style={isDark ? 'light' : 'dark'} />
      {scroll ? (
        <ScrollView
          style={styles.root}
          contentContainerStyle={[inner, { flexGrow: 1 }, contentStyle]}
          keyboardShouldPersistTaps="handled"
          showsVerticalScrollIndicator={false}
          // Dragging the list should dismiss the keyboard, matching both
          // platforms' native behaviour.
          keyboardDismissMode="on-drag"
        >
          {children}
        </ScrollView>
      ) : (
        <View style={[inner, contentStyle]}>{children}</View>
      )}
    </SafeAreaView>
  );
}

const styles = StyleSheet.create({
  root: { flex: 1 },
});
