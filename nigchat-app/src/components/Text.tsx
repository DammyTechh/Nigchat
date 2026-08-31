import React from 'react';
import { Text as RNText, TextProps as RNTextProps, TextStyle } from 'react-native';

import { typography, useColors } from '../theme';

type Variant = keyof typeof typography;
type Tone = 'default' | 'secondary' | 'muted' | 'primary' | 'danger' | 'inverse';

export interface TextProps extends RNTextProps {
  variant?: Variant;
  tone?: Tone;
  /** Centres without needing a style prop at every call site. */
  center?: boolean;
}

/**
 * Every piece of text in the app goes through here.
 *
 * `allowFontScaling` stays on so the app honours the OS text-size setting, but
 * `maxFontSizeMultiplier` is capped: at 300% an unbounded chat row grows tall
 * enough to push the timestamp off screen. 1.6 keeps large-text users
 * comfortable while the layout still holds.
 */
export function Text({ variant = 'body', tone = 'default', center, style, ...rest }: TextProps) {
  const colors = useColors();

  const toneColor: Record<Tone, string> = {
    default: colors.text,
    secondary: colors.textSecondary,
    muted: colors.textMuted,
    primary: colors.primary,
    danger: colors.danger,
    inverse: colors.onPrimary,
  };

  const base: TextStyle = {
    ...typography[variant],
    color: toneColor[tone],
    ...(center ? { textAlign: 'center' } : null),
  };

  return <RNText maxFontSizeMultiplier={1.6} {...rest} style={[base, style]} />;
}
