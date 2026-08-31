import { useRouter } from 'expo-router';
import React from 'react';
import { Alert, StyleSheet, View } from 'react-native';

import { Avatar, Header, Icon, ListRow, Pressable, Screen, Section, Text } from '../../src/components';
import { useAuth } from '../../src/store/auth';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { spacing, useColors, useTheme } from '../../src/theme';
import { prettyE164 } from '../../src/utils/phone';

export default function SettingsScreen() {
  const router = useRouter();
  const colors = useColors();
  const { preference } = useTheme();
  const insets = useSafeAreaInsets();
  const me = useAuth((state) => state.me);
  const signOut = useAuth((state) => state.signOut);

  const themeLabel =
    preference === 'system' ? 'System' : preference === 'dark' ? 'Dark' : 'Light';

  function confirmSignOut() {
    Alert.alert(
      'Sign out?',
      'Your messages stay on this device unless you delete the app. You will need your phone number to sign back in.',
      [
        { text: 'Cancel', style: 'cancel' },
        { text: 'Sign out', style: 'destructive', onPress: () => signOut() },
      ],
    );
  }

  return (
    <Screen edges={['top']} scroll>
      <Header title="You" large borderless />

      <View style={{ paddingHorizontal: spacing.base }}>
        <Pressable onPress={() => {}} style={styles.profile}>
          <Avatar name={me?.display_name ?? '?'} size="xl" />
          <View style={{ flex: 1, gap: 3 }}>
            <Text variant="title" numberOfLines={1}>
              {me?.display_name ?? 'Your name'}
            </Text>
            <Text variant="subhead" tone="muted" numberOfLines={1}>
              {me?.phone_e164 ? prettyE164(me.phone_e164) : ''}
            </Text>
            {me?.about ? (
              <Text variant="footnote" tone="muted" numberOfLines={1}>
                {me.about}
              </Text>
            ) : null}
          </View>
          <Icon name="ChevronRight" size={18} color={colors.textMuted} />
        </Pressable>

        <Section title="Preferences">
          <ListRow
            icon="Palette"
            title="Appearance"
            value={themeLabel}
            chevron
            onPress={() => router.push('/settings/appearance')}
          />
          <ListRow
            icon="Bell"
            title="Notifications"
            subtitle="Tones, quiet hours, previews"
            chevron
            onPress={() => router.push('/settings/notifications')}
          />
          <ListRow icon="Database" title="Storage and data" chevron onPress={() => {}} />
        </Section>

        <Section title="Privacy and security">
          <ListRow
            icon="Lock"
            title="Privacy"
            subtitle="Last seen, read receipts, blocked contacts"
            chevron
            onPress={() => router.push('/settings/privacy')}
          />
          <ListRow
            icon="ShieldCheck"
            title="Security"
            subtitle={me?.two_step_enabled ? 'Two-step verification on' : 'Two-step verification off'}
            chevron
            onPress={() => router.push('/settings/security')}
          />
          <ListRow
            icon="MonitorSmartphone"
            title="Linked devices"
            subtitle="Use NigChat on the web and desktop"
            chevron
            onPress={() => router.push('/settings/devices')}
          />
        </Section>

        <Section title="Support">
          <ListRow icon="CircleHelp" title="Help centre" chevron onPress={() => {}} />
          <ListRow icon="UserPlus" title="Invite a friend" chevron onPress={() => {}} />
        </Section>

        <Section>
          <ListRow icon="LogOut" title="Sign out" danger onPress={confirmSignOut} />
        </Section>

        <Text
          variant="caption"
          tone="muted"
          center
          style={{ marginBottom: 56 + insets.bottom + spacing.xl }}
        >
          NigChat 1.0.0
        </Text>
      </View>
    </Screen>
  );
}

const styles = StyleSheet.create({
  profile: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: spacing.base,
    paddingVertical: spacing.base,
    marginBottom: spacing.lg,
  },
});
