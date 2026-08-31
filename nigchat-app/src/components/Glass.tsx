import { BlurView } from 'expo-blur';
import React from 'react';
import { Platform, StyleSheet, View, ViewStyle } from 'react-native';

import { useTheme } from '../theme';

/**
 * Translucent "glass" surface.
 *
 * The honest engineering position, because this is where cross-platform apps
 * usually go wrong:
 *
 * **iOS** has hardware-accelerated backdrop blur (`UIVisualEffectView`). It is
 * cheap, it is what the OS itself uses, and `BlurView` maps straight onto it.
 *
 * **Android has no equivalent below API 31.** Before Android 12 there is no
 * `RenderEffect`, so any "blur" is either a JS-side downscale-and-redraw — which
 * drops frames the moment a list scrolls behind it — or a fake. Shipping that
 * would make the app feel worse on exactly the devices most of our users have.
 *
 * So the behaviour is tiered:
 *
 *   iOS            → real `UIVisualEffectView` blur
 *   Android 12+    → `RenderEffect` blur via `dimezisBlurView`
 *   Android < 12   → no blur; a near-opaque tinted surface instead
 *
 * The last tier still looks deliberate rather than broken, because the design
 * never depends on blur for legibility — every glass surface carries a tint
 * layer and a hairline underneath. Blur is a finish, not the structure. That is
 * also why text on these surfaces stays readable over a bright photo, which
 * pure transparency cannot guarantee.
 */

const ANDROID_SUPPORTS_BLUR = Platform.OS === 'android' && Number(Platform.Version) >= 31;
export const CAN_BLUR = Platform.OS === 'ios' || ANDROID_SUPPORTS_BLUR;

type Elevation = 'chrome' | 'panel' | 'overlay';

interface GlassProps {
  children?: React.ReactNode;
  /**
   * `chrome`  tab bars and headers — subtle, content reads through
   * `panel`   composer and inline panels — stronger, text sits on it
   * `overlay` sheets and dialogs — heaviest, content behind is dimmed
   */
  elevation?: Elevation;
  style?: ViewStyle;
  /** Hairline on the leading edge. Which edge depends on where it sits. */
  border?: 'top' | 'bottom' | 'none';
}

/** Blur strength per elevation. iOS and Android are scaled differently
 *  because the same numeric intensity reads much heavier on Android. */
const INTENSITY: Record<Elevation, { ios: number; android: number }> = {
  chrome: { ios: 60, android: 40 },
  panel: { ios: 70, android: 48 },
  overlay: { ios: 90, android: 64 },
};

/**
 * Tint sitting on top of the blur.
 *
 * Without it, white text over a light photo becomes unreadable — blur alone
 * reduces detail but not luminance. Apple's own materials do exactly this;
 * "frosted glass" is blur *plus* a translucent fill, never blur by itself.
 */
function tintFor(elevation: Elevation, isDark: boolean, canBlur: boolean): string {
  if (!canBlur) {
    // No blur available: lean almost opaque so contrast is guaranteed.
    return isDark ? 'rgba(17,26,21,0.97)' : 'rgba(255,255,255,0.97)';
  }

  const alpha = { chrome: 0.55, panel: 0.62, overlay: 0.7 }[elevation];
  return isDark ? `rgba(11,18,14,${alpha})` : `rgba(255,255,255,${alpha})`;
}

export function Glass({ children, elevation = 'chrome', style, border = 'none' }: GlassProps) {
  const { theme, isDark } = useTheme();

  const borderStyle: ViewStyle =
    border === 'none'
      ? {}
      : border === 'top'
        ? { borderTopWidth: StyleSheet.hairlineWidth, borderTopColor: theme.colors.border }
        : { borderBottomWidth: StyleSheet.hairlineWidth, borderBottomColor: theme.colors.border };

  const tint = tintFor(elevation, isDark, CAN_BLUR);

  if (!CAN_BLUR) {
    return (
      <View style={[{ backgroundColor: tint }, borderStyle, style]}>{children}</View>
    );
  }

  const intensity =
    Platform.OS === 'ios' ? INTENSITY[elevation].ios : INTENSITY[elevation].android;

  return (
    <BlurView
      intensity={intensity}
      tint={isDark ? 'dark' : 'light'}
      // Opts Android into the RenderEffect path. Ignored on iOS.
      experimentalBlurMethod={ANDROID_SUPPORTS_BLUR ? 'dimezisBlurView' : undefined}
      style={[borderStyle, style]}
    >
      {/* The tint layer, above the blur and below the content. */}
      <View style={[StyleSheet.absoluteFill, { backgroundColor: tint }]} pointerEvents="none" />
      {children}
    </BlurView>
  );
}

/**
 * Full-screen dim behind a sheet or dialog.
 *
 * Blurred on capable devices, plain dim elsewhere — a scrim is the one place a
 * missing blur genuinely does not matter.
 */
export function Scrim({ children, style }: { children?: React.ReactNode; style?: ViewStyle }) {
  const { theme, isDark } = useTheme();

  if (!CAN_BLUR) {
    return (
      <View style={[StyleSheet.absoluteFill, { backgroundColor: theme.colors.scrim }, style]}>
        {children}
      </View>
    );
  }

  return (
    <BlurView
      intensity={Platform.OS === 'ios' ? 24 : 18}
      tint={isDark ? 'dark' : 'light'}
      experimentalBlurMethod={ANDROID_SUPPORTS_BLUR ? 'dimezisBlurView' : undefined}
      style={[StyleSheet.absoluteFill, style]}
    >
      <View
        style={[StyleSheet.absoluteFill, { backgroundColor: theme.colors.scrim }]}
        pointerEvents="none"
      />
      {children}
    </BlurView>
  );
}
