import * as LucideIcons from 'lucide-react-native';
import React from 'react';

import { useColors } from '../theme';

/**
 * Icons come from Lucide — real vector icons with a consistent 24px grid and
 * 2px stroke, not emoji and not a mixed bag scraped from different sets.
 * Consistent optical weight across every icon is one of the quiet things that
 * makes an interface feel finished.
 */
export type IconName = keyof typeof LucideIcons;

interface IconProps {
  name: IconName;
  size?: number;
  color?: string;
  /** Thinner for large decorative icons, heavier for small dense ones. */
  strokeWidth?: number;
  fill?: string;
}

export function Icon({ name, size = 22, color, strokeWidth = 2, fill = 'none' }: IconProps) {
  const colors = useColors();
  const Component = LucideIcons[name] as React.ComponentType<{
    size?: number;
    color?: string;
    strokeWidth?: number;
    fill?: string;
  }>;

  if (!Component) {
    if (__DEV__) {
      console.warn(`Icon "${String(name)}" does not exist in lucide-react-native`);
    }
    return null;
  }

  return (
    <Component
      size={size}
      color={color ?? colors.text}
      strokeWidth={strokeWidth}
      fill={fill}
    />
  );
}
