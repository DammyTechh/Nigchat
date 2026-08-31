import React from 'react';
import { StyleSheet, View } from 'react-native';

import { Header, Icon, ListRow, MessageBubble, Screen, Section, Text } from '../../src/components';
import { radius, spacing, ThemePreference, useColors, useTheme } from '../../src/theme';

const OPTIONS: { value: ThemePreference; label: string; description: string; icon: 'Smartphone' | 'Sun' | 'Moon' }[] = [
  { value: 'system', label: 'System', description: 'Match your device setting', icon: 'Smartphone' },
  { value: 'light', label: 'Light', description: 'Always light', icon: 'Sun' },
  { value: 'dark', label: 'Dark', description: 'Always dark', icon: 'Moon' },
];

export default function AppearanceScreen() {
  const colors = useColors();
  const { preference, setPreference } = useTheme();

  return (
    <Screen edges={['top', 'bottom']} scroll>
      <Header title="Appearance" back />

      <View style={{ paddingHorizontal: spacing.base, paddingTop: spacing.base }}>
        {/* A live preview, not a swatch. Theme choices are hard to judge in the
            abstract; showing real bubbles answers the question immediately. */}
        <View style={[styles.preview, { backgroundColor: colors.surfaceRaised, borderColor: colors.border }]}>
          <MessageBubble body="Are we still on for 6?" time="09:24" outgoing={false} />
          <MessageBubble body="Yes — see you there" time="09:25" outgoing state="read" />
        </View>

        <Section title="Theme">
          {OPTIONS.map((option) => (
            <ListRow
              key={option.value}
              icon={option.icon}
              title={option.label}
              subtitle={option.description}
              onPress={() => setPreference(option.value)}
              right={
                preference === option.value ? (
                  <Icon name="Check" size={19} color={colors.primary} />
                ) : null
              }
            />
          ))}
        </Section>

        <Text variant="footnote" tone="muted" style={{ marginHorizontal: spacing.xs }}>
          Dark mode uses a deep green-black rather than pure black, which avoids the
          smearing OLED screens show when a list scrolls over solid black.
        </Text>
      </View>
    </Screen>
  );
}

const styles = StyleSheet.create({
  preview: {
    borderRadius: radius.lg,
    borderWidth: StyleSheet.hairlineWidth,
    paddingVertical: spacing.base,
    marginBottom: spacing.xl,
  },
});
