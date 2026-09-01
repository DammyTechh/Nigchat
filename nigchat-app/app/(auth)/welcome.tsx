import { Image } from 'expo-image';
import { useRouter } from 'expo-router';
import React from 'react';
import { StyleSheet, useWindowDimensions, View } from 'react-native';

import { Button, Icon, Screen, Text } from '../../src/components';
import { spacing, useColors } from '../../src/theme';

/**
 * Every claim here is one the product actually delivers today.
 *
 * The previous version said "Every message is encrypted on your device. Not even
 * we can read them." That is not true yet — the transport carries base64, which
 * is encoding, not encryption. Shipping a privacy claim you cannot back is how
 * a small app ends up in a story it does not want to be in, so it is gone until
 * the Signal layer is real.
 */
const HIGHLIGHTS = [
  {
    icon: 'SignalHigh' as const,
    title: 'Works on one bar',
    body: 'Send on a weak signal. Your message arrives once, even if the app retries.',
  },
  {
    icon: 'EyeOff' as const,
    title: 'You decide what shows',
    body: 'Hide when you were last online. Read messages without a receipt going back.',
  },
  {
    icon: 'MonitorSmartphone' as const,
    title: 'Phone and laptop',
    body: 'Scan a code and carry the same conversation to a bigger screen.',
  },
];

export default function Welcome() {
  const router = useRouter();
  const colors = useColors();
  const { height } = useWindowDimensions();

  // On a small phone the three highlights would push the button below the fold.
  const compact = height < 700;

  return (
    <Screen scroll padded edges={['top', 'bottom']}>
      <View style={styles.hero}>
        <Image
          source={require('../../assets/logo-full.png')}
          style={{ width: compact ? 150 : 190, height: compact ? 145 : 183 }}
          contentFit="contain"
        />
      </View>

      <View style={{ gap: compact ? spacing.base : spacing.lg }}>
        {HIGHLIGHTS.slice(0, compact ? 2 : 3).map((item) => (
          <View key={item.title} style={styles.row}>
            <View style={[styles.iconTile, { backgroundColor: colors.primarySoft }]}>
              <Icon name={item.icon} size={19} color={colors.primary} strokeWidth={1.9} />
            </View>
            <View style={{ flex: 1 }}>
              <Text variant="headline">{item.title}</Text>
              <Text variant="footnote" tone="muted" style={{ marginTop: 2 }}>
                {item.body}
              </Text>
            </View>
          </View>
        ))}
      </View>

      <View style={styles.footer}>
        <Button
          label="Get started"
          fullWidth
          size="lg"
          onPress={() => router.push('/(auth)/phone')}
        />
        <Text variant="caption" tone="muted" center style={{ marginTop: spacing.md }}>
          By continuing you agree to the Terms of Service and Privacy Policy.
        </Text>
      </View>
    </Screen>
  );
}

const styles = StyleSheet.create({
  hero: { alignItems: 'center', justifyContent: 'center', paddingVertical: spacing.xxl, flex: 1 },
  row: { flexDirection: 'row', gap: spacing.base, alignItems: 'flex-start' },
  iconTile: {
    width: 38,
    height: 38,
    borderRadius: 12,
    alignItems: 'center',
    justifyContent: 'center',
  },
  footer: { paddingTop: spacing.xxl, paddingBottom: spacing.base },
});
