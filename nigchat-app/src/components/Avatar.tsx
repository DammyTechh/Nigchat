import { Image } from 'expo-image';
import React, { useMemo } from 'react';
import { StyleSheet, View } from 'react-native';

import { layout, useColors } from '../theme';
import { Text } from './Text';

type Size = keyof typeof layout.avatar;

interface AvatarProps {
  name: string;
  uri?: string | null;
  size?: Size | number;
  /** Green dot for presence. */
  online?: boolean;
  /** Unread stories ring, used on the Updates screen. */
  ring?: 'none' | 'unseen' | 'seen';
}

/**
 * Deterministic colour from the name, so the same person keeps the same tile
 * across devices and sessions. Six muted greens and neutrals rather than the
 * usual saturated rainbow — a wall of bright circles is the fastest way to make
 * a list look cheap.
 */
const TILES = ['#0E7A46', '#2F6E52', '#4A6B5C', '#3E7A63', '#1F5E43', '#557066'];

function initials(name: string) {
  const words = name.trim().split(/\s+/).filter(Boolean);
  if (words.length === 0) return '?';
  if (words.length === 1) return words[0].slice(0, 2).toUpperCase();
  return (words[0][0] + words[words.length - 1][0]).toUpperCase();
}

function tileFor(name: string) {
  let hash = 0;
  for (let i = 0; i < name.length; i += 1) {
    hash = (hash * 31 + name.charCodeAt(i)) >>> 0;
  }
  return TILES[hash % TILES.length];
}

export function Avatar({ name, uri, size = 'lg', online, ring = 'none' }: AvatarProps) {
  const colors = useColors();
  const dimension = typeof size === 'number' ? size : layout.avatar[size];
  const background = useMemo(() => tileFor(name), [name]);
  const ringWidth = ring === 'none' ? 0 : 2;
  const gap = ring === 'none' ? 0 : 3;
  const outer = dimension + (ringWidth + gap) * 2;

  const content = uri ? (
    <Image
      source={{ uri }}
      style={{ width: dimension, height: dimension, borderRadius: dimension / 2 }}
      contentFit="cover"
      // Blurhash-style placeholder avoids the grey flash while a photo loads.
      transition={160}
    />
  ) : (
    <View
      style={[
        styles.fallback,
        { width: dimension, height: dimension, borderRadius: dimension / 2, backgroundColor: background },
      ]}
    >
      <Text
        style={{ color: '#FFFFFF', fontSize: dimension * 0.36, fontWeight: '600' }}
        allowFontScaling={false}
      >
        {initials(name)}
      </Text>
    </View>
  );

  return (
    <View style={{ width: outer, height: outer, alignItems: 'center', justifyContent: 'center' }}>
      {ring !== 'none' && (
        <View
          style={[
            StyleSheet.absoluteFillObject,
            {
              borderRadius: outer / 2,
              borderWidth: ringWidth,
              borderColor: ring === 'unseen' ? colors.primary : colors.border,
            },
          ]}
        />
      )}
      {content}
      {online && (
        <View
          style={[
            styles.presence,
            {
              width: dimension * 0.28,
              height: dimension * 0.28,
              borderRadius: dimension * 0.14,
              backgroundColor: colors.online,
              borderColor: colors.background,
            },
          ]}
        />
      )}
    </View>
  );
}

const styles = StyleSheet.create({
  fallback: { alignItems: 'center', justifyContent: 'center' },
  presence: {
    position: 'absolute',
    right: 0,
    bottom: 0,
    borderWidth: 2,
  },
});
