import * as Localization from 'expo-localization';
import React, { useEffect, useState } from 'react';
import { ScrollView, StyleSheet, View } from 'react-native';

import { notifications as notificationsApi } from '../../src/api/endpoints';
import type { NotificationPreferences, NotificationTone } from '../../src/api/types';
import {
  Header,
  Icon,
  ListRow,
  Screen,
  Section,
  SkeletonRow,
  Text,
} from '../../src/components';
import { radius, spacing, useColors } from '../../src/theme';
import { minutesToClock } from '../../src/utils/format';

/**
 * Notification settings.
 *
 * Every control here maps to a rule the server actually enforces, so the screen
 * is written to explain *behaviour*, not just expose switches. Two are worth
 * calling out in the UI because users otherwise assume the opposite:
 *
 *   - a muted chat still notifies you when someone @mentions you
 *   - security alerts cannot be turned off, by design
 */
export default function NotificationSettings() {
  const colors = useColors();

  const [prefs, setPrefs] = useState<NotificationPreferences | null>(null);
  const [tones, setTones] = useState<NotificationTone[]>([]);
  const [picking, setPicking] = useState<null | 'message' | 'group' | 'call'>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    Promise.all([notificationsApi.preferences(), notificationsApi.tones()])
      .then(([preferences, toneList]) => {
        setPrefs(preferences);
        setTones(toneList);
      })
      .catch(() => {});
  }, []);

  async function update(patch: Partial<NotificationPreferences>) {
    if (!prefs) return;

    // Optimistic: a settings toggle that waits for a round trip feels broken on
    // a slow connection.
    const previous = prefs;
    setPrefs({ ...prefs, ...patch });
    setSaving(true);

    try {
      const saved = await notificationsApi.updatePreferences(patch);
      setPrefs(saved);
    } catch {
      setPrefs(previous);
    } finally {
      setSaving(false);
    }
  }

  function toneName(id: string | null) {
    return tones.find((tone) => tone.id === id)?.display_name ?? 'Default';
  }

  if (!prefs) {
    return (
      <Screen edges={['top', 'bottom']}>
        <Header title="Notifications" back />
        {Array.from({ length: 6 }).map((_, index) => (
          <SkeletonRow key={index} />
        ))}
      </Screen>
    );
  }

  if (picking) {
    const category = picking === 'call' ? 'call' : picking;
    const available = tones.filter((tone) => tone.category === category);
    const selectedId =
      picking === 'message'
        ? prefs.message_tone_id
        : picking === 'group'
          ? prefs.group_tone_id
          : prefs.call_ringtone_id;

    return (
      <Screen edges={['top', 'bottom']}>
        <Header
          title={picking === 'call' ? 'Ringtone' : picking === 'group' ? 'Group tone' : 'Message tone'}
          back
          onBack={() => setPicking(null)}
        />
        <ScrollView contentContainerStyle={{ padding: spacing.base }}>
          <Section>
            {available.map((tone) => (
              <ListRow
                key={tone.id}
                title={tone.display_name}
                subtitle={tone.is_default ? 'Default' : undefined}
                onPress={() => {
                  const field =
                    picking === 'message'
                      ? 'message_tone_id'
                      : picking === 'group'
                        ? 'group_tone_id'
                        : 'call_ringtone_id';
                  update({ [field]: tone.id } as Partial<NotificationPreferences>);
                  setPicking(null);
                }}
                right={
                  selectedId === tone.id ? (
                    <Icon name="Check" size={19} color={colors.primary} />
                  ) : (
                    <Icon name="Play" size={16} color={colors.textMuted} />
                  )
                }
              />
            ))}
          </Section>
          <Text variant="footnote" tone="muted" style={{ marginHorizontal: spacing.xs }}>
            Tones play from the app on this device. Any chat can override this with its own
            sound from that chat&apos;s settings.
          </Text>
        </ScrollView>
      </Screen>
    );
  }

  const quiet = prefs.quiet_hours;

  return (
    <Screen edges={['top', 'bottom']} scroll>
      <Header title="Notifications" back />

      <View style={{ paddingHorizontal: spacing.base, paddingTop: spacing.base }}>
        <Section title="Alerts">
          <ListRow
            icon="MessageSquare"
            title="Direct messages"
            toggle={{
              value: prefs.messages_enabled,
              onChange: (value) => update({ messages_enabled: value }),
            }}
          />
          <ListRow
            icon="Users"
            title="Groups"
            toggle={{
              value: prefs.groups_enabled,
              onChange: (value) => update({ groups_enabled: value }),
            }}
          />
          <ListRow
            icon="Phone"
            title="Calls"
            toggle={{
              value: prefs.calls_enabled,
              onChange: (value) => update({ calls_enabled: value }),
            }}
          />
          <ListRow
            icon="CircleDashed"
            title="Status updates"
            toggle={{
              value: prefs.status_enabled,
              onChange: (value) => update({ status_enabled: value }),
            }}
          />
          <ListRow
            icon="Heart"
            title="Reactions"
            toggle={{
              value: prefs.reactions_enabled,
              onChange: (value) => update({ reactions_enabled: value }),
            }}
          />
        </Section>

        <Section
          title="Sounds"
          footer="A chat you have muted will still notify you when someone mentions you by name."
        >
          <ListRow
            icon="Music"
            title="Message tone"
            value={toneName(prefs.message_tone_id)}
            chevron
            onPress={() => setPicking('message')}
          />
          <ListRow
            icon="Music4"
            title="Group tone"
            value={toneName(prefs.group_tone_id)}
            chevron
            onPress={() => setPicking('group')}
          />
          <ListRow
            icon="BellRing"
            title="Ringtone"
            value={toneName(prefs.call_ringtone_id)}
            chevron
            onPress={() => setPicking('call')}
          />
          <ListRow
            icon="Vibrate"
            title="Vibration"
            value={prefs.vibration === 'off' ? 'Off' : prefs.vibration === 'short' ? 'Short' : prefs.vibration === 'long' ? 'Long' : 'Default'}
            chevron
            onPress={() =>
              update({
                vibration:
                  prefs.vibration === 'off'
                    ? 'short'
                    : prefs.vibration === 'short'
                      ? 'default'
                      : prefs.vibration === 'default'
                        ? 'long'
                        : 'off',
              })
            }
          />
          <ListRow
            icon="Volume2"
            title="In-app sounds"
            toggle={{
              value: prefs.in_app_sounds,
              onChange: (value) => update({ in_app_sounds: value }),
            }}
          />
        </Section>

        <Section
          title="Quiet hours"
          footer={
            quiet
              ? `Notifications are silent between ${minutesToClock(quiet.start_minute)} and ${minutesToClock(quiet.end_minute)} in your local time (${quiet.timezone}). ${quiet.allow_calls ? 'Calls can still ring.' : 'Calls are silent too.'}`
              : 'Silence notifications during set hours. Your local time is used, so the window follows you when you travel.'
          }
        >
          <ListRow
            icon="MoonStar"
            title="Quiet hours"
            toggle={{
              value: !!quiet,
              onChange: (value) =>
                update({
                  quiet_hours: value
                    ? {
                        // 22:00–07:00 as a sensible default, in the device's
                        // own zone rather than the server's.
                        start_minute: 22 * 60,
                        end_minute: 7 * 60,
                        timezone: Localization.getCalendars()[0]?.timeZone ?? 'UTC',
                        allow_calls: true,
                      }
                    : null,
                }),
            }}
          />
          {quiet ? (
            <ListRow
              icon="PhoneCall"
              title="Let calls through"
              subtitle="Messages stay silent"
              toggle={{
                value: quiet.allow_calls,
                onChange: (value) =>
                  update({ quiet_hours: { ...quiet, allow_calls: value } }),
              }}
            />
          ) : null}
        </Section>

        <Section
          title="Previews"
          footer="Choose how much appears on your lock screen. With previews hidden, the notification is delivered without any message content — the phone shows only that something arrived."
        >
          {(['full', 'name_only', 'hidden'] as const).map((mode) => (
            <ListRow
              key={mode}
              title={
                mode === 'full' ? 'Name and message' : mode === 'name_only' ? 'Name only' : 'Nothing'
              }
              onPress={() => update({ preview_mode: mode })}
              right={
                prefs.preview_mode === mode ? (
                  <Icon name="Check" size={19} color={colors.primary} />
                ) : null
              }
            />
          ))}
        </Section>

        <View style={[styles.notice, { backgroundColor: colors.primarySoft }]}>
          <Icon name="ShieldCheck" size={17} color={colors.primary} />
          <Text variant="footnote" style={{ flex: 1, color: colors.text }}>
            Security alerts — a new device signing in, or a contact&apos;s security code
            changing — always come through, even during quiet hours.
          </Text>
        </View>
      </View>
    </Screen>
  );
}

const styles = StyleSheet.create({
  notice: {
    flexDirection: 'row',
    gap: spacing.md,
    padding: spacing.base,
    borderRadius: radius.md,
    marginBottom: spacing.xxl,
    alignItems: 'flex-start',
  },
});
