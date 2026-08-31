import React, { useEffect, useState } from 'react';
import { View } from 'react-native';

import { Alert } from 'react-native';

import { Header, Icon, ListRow, Screen, Section, Text } from '../../src/components';
import { appLock, getCapability } from '../../src/utils/biometrics';
import { spacing, useColors } from '../../src/theme';

type Visibility = 'everyone' | 'contacts' | 'nobody';

const LABELS: Record<Visibility, string> = {
  everyone: 'Everyone',
  contacts: 'My contacts',
  nobody: 'Nobody',
};

export default function PrivacyScreen() {
  const colors = useColors();
  const [lastSeen, setLastSeen] = useState<Visibility>('contacts');
  const [photo, setPhoto] = useState<Visibility>('contacts');
  const [about, setAbout] = useState<Visibility>('contacts');
  const [readReceipts, setReadReceipts] = useState(true);
  const [typing, setTyping] = useState(true);
  const [groups, setGroups] = useState<Visibility>('contacts');

  // Biometrics are a device capability, not an account setting — read the
  // hardware rather than assuming Face ID exists.
  const [lockEnabled, setLockEnabled] = useState(false);
  const [biometric, setBiometric] = useState({ available: false, label: 'your screen lock' });

  useEffect(() => {
    getCapability().then((capability) =>
      setBiometric({ available: capability.available, label: capability.label }),
    );
    appLock.isEnabled().then(setLockEnabled);
  }, []);

  async function toggleAppLock(next: boolean) {
    if (!biometric.available) {
      Alert.alert(
        'Not available',
        'Set a screen lock — Face ID, a fingerprint, a pattern or a PIN — in your phone settings first.',
      );
      return;
    }
    // Both directions require a successful scan. Allowing either without one
    // would make the lock theatre.
    const ok = next ? await appLock.enable() : await appLock.disable();
    if (ok) setLockEnabled(next);
  }

  const cycle = (value: Visibility): Visibility =>
    value === 'everyone' ? 'contacts' : value === 'contacts' ? 'nobody' : 'everyone';

  return (
    <Screen edges={['top', 'bottom']} scroll>
      <Header title="Privacy" back />

      <View style={{ paddingHorizontal: spacing.base, paddingTop: spacing.base }}>
        <Section title="Who can see">
          <ListRow
            icon="Eye"
            title="Last seen and online"
            value={LABELS[lastSeen]}
            chevron
            onPress={() => setLastSeen(cycle(lastSeen))}
          />
          <ListRow
            icon="Image"
            title="Profile photo"
            value={LABELS[photo]}
            chevron
            onPress={() => setPhoto(cycle(photo))}
          />
          <ListRow
            icon="Info"
            title="About"
            value={LABELS[about]}
            chevron
            onPress={() => setAbout(cycle(about))}
          />
          <ListRow
            icon="UsersRound"
            title="Adding to groups"
            value={LABELS[groups]}
            chevron
            onPress={() => setGroups(cycle(groups))}
          />
        </Section>

        <Section
          title="Activity"
          footer="Turning off read receipts also means you cannot see when others have read your messages. It does not apply to group chats."
        >
          <ListRow
            icon="CheckCheck"
            title="Read receipts"
            toggle={{ value: readReceipts, onChange: setReadReceipts }}
          />
          <ListRow
            icon="PenLine"
            title="Typing indicators"
            toggle={{ value: typing, onChange: setTyping }}
          />
        </Section>

        <Section
          title="Device lock"
          footer={
            biometric.available
              ? `${biometric.label} protects this device only. It is not a second password for your account, and it does not change how your messages are encrypted.`
              : 'Set a screen lock in your phone settings to use app lock.'
          }
        >
          <ListRow
            icon="Fingerprint"
            title={`Unlock with ${biometric.label}`}
            subtitle="Ask every time you open NigChat"
            toggle={{ value: lockEnabled, onChange: toggleAppLock }}
          />
          <ListRow
            icon="Lock"
            title="Locked chats"
            subtitle="Hide individual chats behind a scan"
            chevron
            onPress={() => {}}
          />
        </Section>

        <Section title="Contacts">
          <ListRow icon="UserX" title="Blocked contacts" value="0" chevron onPress={() => {}} />
        </Section>

        <View style={{ flexDirection: 'row', gap: spacing.md, marginBottom: spacing.xxl }}>
          <Icon name="Lock" size={16} color={colors.textMuted} />
          <Text variant="footnote" tone="muted" style={{ flex: 1 }}>
            Your messages are end-to-end encrypted regardless of these settings. They control
            what other people can see about you, not what we can read — we cannot read any of it.
          </Text>
        </View>
      </View>
    </Screen>
  );
}
