import { useRouter } from 'expo-router';
import React, { useEffect, useState } from 'react';
import { View } from 'react-native';

import { users as usersApi } from '../../src/api/endpoints';
import type { SecurityEvent } from '../../src/api/types';
import { Header, Icon, ListRow, Screen, Section, Text } from '../../src/components';
import { useAuth } from '../../src/store/auth';
import { radius, spacing, useColors } from '../../src/theme';
import { listTimestamp } from '../../src/utils/format';

const EVENT_LABELS: Record<string, string> = {
  login: 'Signed in',
  logout: 'Signed out',
  device_linked: 'New device linked',
  device_revoked: 'Device signed out',
  key_changed: 'Security code changed',
  pin_changed: 'Two-step PIN changed',
  pin_failed: 'Incorrect PIN entered',
  session_reuse_detected: 'Suspicious session activity',
  suspicious_login: 'Unusual sign-in',
  two_step_enabled: 'Two-step verification enabled',
  two_step_disabled: 'Two-step verification disabled',
};

export default function SecurityScreen() {
  const router = useRouter();
  const colors = useColors();
  const me = useAuth((state) => state.me);
  const [events, setEvents] = useState<SecurityEvent[]>([]);

  useEffect(() => {
    usersApi.securityEvents().then(setEvents).catch(() => {});
  }, []);

  return (
    <Screen edges={['top', 'bottom']} scroll>
      <Header title="Security" back />

      <View style={{ paddingHorizontal: spacing.base, paddingTop: spacing.base }}>
        <View style={[styles(colors).banner]}>
          <Icon name="ShieldCheck" size={20} color={colors.primary} />
          <Text variant="footnote" style={{ flex: 1 }}>
            Messages and calls are end-to-end encrypted. Keys never leave your devices, so
            nobody — including us — can read what you send.
          </Text>
        </View>

        <Section
          title="Account protection"
          footer="Two-step verification stops someone taking your account with a hijacked SIM alone. Turn it on."
        >
          <ListRow
            icon="KeyRound"
            title="Two-step verification"
            subtitle={me?.two_step_enabled ? 'On' : 'Off — recommended'}
            value={me?.two_step_enabled ? undefined : 'Set up'}
            chevron
            onPress={() => router.push('/settings/two-step')}
          />
          <ListRow
            icon="Fingerprint"
            title="Passkey"
            subtitle="Sign in with Face ID instead of a code"
            chevron
            onPress={() => {}}
          />
          <ListRow
            icon="BellRing"
            title="Security notifications"
            subtitle="Always on — cannot be disabled"
            right={<Icon name="Lock" size={15} color={colors.textMuted} />}
          />
        </Section>

        <Section title="Recent activity" footer="Anything you do not recognise is worth acting on.">
          {events.length === 0 ? (
            <ListRow title="Nothing yet" subtitle="Sign-ins and key changes will appear here" />
          ) : (
            events.slice(0, 12).map((event, index) => (
              <ListRow
                key={index}
                icon={
                  event.severity === 'critical'
                    ? 'TriangleAlert'
                    : event.severity === 'warning'
                      ? 'CircleAlert'
                      : 'Check'
                }
                title={EVENT_LABELS[event.event_type] ?? event.event_type}
                subtitle={listTimestamp(event.created_at)}
                danger={event.severity === 'critical'}
              />
            ))
          )}
        </Section>
      </View>
    </Screen>
  );
}

const styles = (colors: ReturnType<typeof useColors>) => ({
  banner: {
    flexDirection: 'row' as const,
    gap: spacing.md,
    alignItems: 'flex-start' as const,
    padding: spacing.base,
    borderRadius: radius.md,
    backgroundColor: colors.primarySoft,
    marginBottom: spacing.xl,
  },
});
