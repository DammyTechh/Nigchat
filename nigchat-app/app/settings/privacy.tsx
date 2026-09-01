import React, { useEffect, useState } from 'react';
import { View } from 'react-native';

import { users as usersApi } from '../../src/api/endpoints';
import type { PrivacySettings, Visibility } from '../../src/api/types';
import {
  Header,
  Icon,
  ListRow,
  Screen,
  Section,
  SkeletonRow,
  Text,
} from '../../src/components';
import { appLock, getCapability } from '../../src/utils/biometrics';
import { spacing, useColors } from '../../src/theme';
import { Alert } from 'react-native';

const LABELS: Record<Visibility, string> = {
  everyone: 'Everyone',
  contacts: 'My contacts',
  nobody: 'Nobody',
};

/** Cycles everyone → contacts → nobody. A picker sheet is the right long-term
 *  control, but three states cycle acceptably and avoid a modal for one tap. */
function next(value: Visibility): Visibility {
  return value === 'everyone' ? 'contacts' : value === 'contacts' ? 'nobody' : 'everyone';
}

export default function PrivacyScreen() {
  const colors = useColors();

  const [settings, setSettings] = useState<PrivacySettings | null>(null);
  const [lockEnabled, setLockEnabled] = useState(false);
  const [biometric, setBiometric] = useState({ available: false, label: 'your screen lock' });

  useEffect(() => {
    usersApi.privacy().then(setSettings).catch(() => {});
    getCapability().then((c) => setBiometric({ available: c.available, label: c.label }));
    appLock.isEnabled().then(setLockEnabled);
  }, []);

  /**
   * Optimistic, then reconciled with what the server returns.
   *
   * A settings toggle that waits for a round trip feels broken on a slow
   * connection, and these are enforced server-side anyway — the response is
   * the truth, so a rejected change simply snaps back.
   */
  async function update(patch: Partial<PrivacySettings>) {
    if (!settings) return;
    const previous = settings;
    setSettings({ ...settings, ...patch });

    try {
      setSettings(await usersApi.updatePrivacy(patch));
    } catch {
      setSettings(previous);
    }
  }

  async function toggleAppLock(on: boolean) {
    if (!biometric.available) {
      Alert.alert(
        'Not available',
        'Set a screen lock — Face ID, a fingerprint, a pattern or a PIN — in your phone settings first.',
      );
      return;
    }
    const ok = on ? await appLock.enable() : await appLock.disable();
    if (ok) setLockEnabled(on);
  }

  if (!settings) {
    return (
      <Screen edges={['top', 'bottom']}>
        <Header title="Privacy" back />
        {Array.from({ length: 6 }).map((_, i) => (
          <SkeletonRow key={i} />
        ))}
      </Screen>
    );
  }

  return (
    <Screen edges={['top', 'bottom']} scroll>
      <Header title="Privacy" back />

      <View style={{ paddingHorizontal: spacing.base, paddingTop: spacing.base }}>
        <Section
          title="Who can see"
          footer="These are applied on the server, not by the app. Someone running a modified client still cannot see what you have hidden — it is never sent to them."
        >
          <ListRow
            icon="Eye"
            title="Last seen and online"
            value={LABELS[settings.last_seen]}
            chevron
            onPress={() => update({ last_seen: next(settings.last_seen) })}
          />
          <ListRow
            icon="Image"
            title="Profile photo"
            value={LABELS[settings.profile_photo]}
            chevron
            onPress={() => update({ profile_photo: next(settings.profile_photo) })}
          />
          <ListRow
            icon="Info"
            title="About"
            value={LABELS[settings.about]}
            chevron
            onPress={() => update({ about: next(settings.about) })}
          />
          <ListRow
            icon="CircleDashed"
            title="Status"
            value={LABELS[settings.status]}
            chevron
            onPress={() => update({ status: next(settings.status) })}
          />
          <ListRow
            icon="UsersRound"
            title="Adding me to groups"
            value={LABELS[settings.who_can_add_to_groups]}
            chevron
            onPress={() => update({ who_can_add_to_groups: next(settings.who_can_add_to_groups) })}
          />
        </Section>

        <Section
          title="Activity"
          footer="With read receipts off, nobody sees when you have read a message — and you stop seeing when they have read yours. The trade is deliberate and symmetric. It does not apply to group chats."
        >
          <ListRow
            icon="CheckCheck"
            title="Read receipts"
            subtitle="Let people see when you have read their message"
            toggle={{
              value: settings.read_receipts_enabled,
              onChange: (v) => update({ read_receipts_enabled: v }),
            }}
          />
          <ListRow
            icon="PenLine"
            title="Typing indicators"
            subtitle="Show when you are writing"
            toggle={{
              value: settings.typing_indicators_enabled,
              onChange: (v) => update({ typing_indicators_enabled: v }),
            }}
          />
        </Section>

        <Section title="Calls">
          <ListRow
            icon="Phone"
            title="Who can call me"
            value={LABELS[settings.who_can_call]}
            chevron
            onPress={() => update({ who_can_call: next(settings.who_can_call) })}
          />
          <ListRow
            icon="BellOff"
            title="Silence unknown callers"
            subtitle="They still appear in your call list"
            toggle={{
              value: settings.silence_unknown_callers,
              onChange: (v) => update({ silence_unknown_callers: v }),
            }}
          />
        </Section>

        <Section
          title="Device lock"
          footer={
            biometric.available
              ? `${biometric.label} protects this device only. It is not a second password for your account, and it does not change how messages are encrypted.`
              : 'Set a screen lock in your phone settings to use app lock.'
          }
        >
          <ListRow
            icon="Fingerprint"
            title={`Unlock with ${biometric.label}`}
            subtitle="Ask every time you open NigChat"
            toggle={{ value: lockEnabled, onChange: toggleAppLock }}
          />
        </Section>

        <View style={{ flexDirection: 'row', gap: spacing.md, marginBottom: spacing.xxl }}>
          <Icon name="Lock" size={16} color={colors.textMuted} />
          <Text variant="footnote" tone="muted" style={{ flex: 1 }}>
            These settings control what other people can see about you. They are separate
            from message encryption, which applies regardless.
          </Text>
        </View>
      </View>
    </Screen>
  );
}
