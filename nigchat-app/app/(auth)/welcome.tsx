import { Image } from 'expo-image';
import { useRouter } from 'expo-router';
import React from 'react';
import { StyleSheet, useWindowDimensions, View } from 'react-native';

import { Button, Icon, Screen, Text } from '../../src/components';
import { spacing, useColors } from '../../src/theme';

const HIGHLIGHTS = [
  {
    icon: 'ShieldCheck' as const,
    title: 'Private by default',
    body: 'Every message is encrypted on your device. Not even we can read them.',
  },
  {
    icon: 'Zap' as const,
    title: 'Built for real networks',
    body: 'Messages send on one bar and arrive once — never twice.',
  },
  {
    icon: 'MonitorSmartphone' as const,
    title: 'Continue anywhere',
    body: 'Scan a code to carry the same conversation onto your laptop.',
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
          source={require('../../assets/images/logo-full.png')}
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
