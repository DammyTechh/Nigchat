import { useFocusEffect, useRouter } from 'expo-router';
import React, { useCallback, useState } from 'react';
import { Alert, StyleSheet, View } from 'react-native';

import { devices as devicesApi } from '../../src/api/endpoints';
import type { Device } from '../../src/api/types';
import {
  Button,
  Header,
  Icon,
  ListRow,
  Screen,
  Section,
  SkeletonRow,
  Text,
} from '../../src/components';
import { radius, spacing, useColors } from '../../src/theme';

const PLATFORM_ICONS: Record<string, 'Smartphone' | 'Tablet' | 'Monitor' | 'Globe' | 'Laptop'> = {
  ios: 'Smartphone',
  android: 'Smartphone',
  ipados: 'Tablet',
  android_tablet: 'Tablet',
  web: 'Globe',
  windows: 'Monitor',
  macos: 'Laptop',
  linux: 'Monitor',
};

function platformLabel(platform: string) {
  const labels: Record<string, string> = {
    ios: 'iPhone',
    ipados: 'iPad',
    android: 'Android phone',
    android_tablet: 'Android tablet',
    web: 'Web browser',
    windows: 'Windows',
    macos: 'Mac',
    linux: 'Linux',
  };
  return labels[platform] ?? platform;
}

function lastActive(iso: string | null) {
  if (!iso) return 'Never used';
  const minutes = Math.round((Date.now() - new Date(iso).getTime()) / 60000);
  if (minutes < 2) return 'Active now';
  if (minutes < 60) return `Active ${minutes}m ago`;
  if (minutes < 1440) return `Active ${Math.round(minutes / 60)}h ago`;
  return `Active ${Math.round(minutes / 1440)}d ago`;
}

export default function DevicesScreen() {
  const router = useRouter();
  const colors = useColors();
  const [devices, setDevices] = useState<Device[] | null>(null);

  const load = useCallback(() => {
    devicesApi
      .list()
      .then(setDevices)
      .catch(() => setDevices([]));
  }, []);

  useFocusEffect(
    useCallback(() => {
      load();
    }, [load]),
  );

  function confirmRevoke(device: Device) {
    Alert.alert(
      `Sign out ${device.device_name ?? platformLabel(device.platform)}?`,
      'That device will be signed out immediately and will stop receiving messages.',
      [
        { text: 'Cancel', style: 'cancel' },
        {
          text: 'Sign out',
          style: 'destructive',
          onPress: async () => {
            await devicesApi.revoke(device.id).catch(() => {});
            load();
          },
        },
      ],
    );
  }

  return (
    <Screen edges={['top', 'bottom']} scroll>
      <Header title="Linked devices" back />

      <View style={{ paddingHorizontal: spacing.base, paddingTop: spacing.base }}>
        <View style={[styles.hero, { backgroundColor: colors.primarySoft }]}>
          <View style={[styles.heroIcon, { backgroundColor: colors.primary }]}>
            <Icon name="QrCode" size={22} color={colors.onPrimary} />
          </View>
          <Text variant="titleSmall" center style={{ marginTop: spacing.md }}>
            Continue on the web
          </Text>
          <Text variant="footnote" tone="muted" center style={{ marginTop: 4, maxWidth: 300 }}>
            Open nigchat.com on your computer, then scan the code it shows. Your chats stay
            encrypted — this phone keeps the keys.
          </Text>
          <Button
            label="Scan code"
            icon="Scan"
            onPress={() => router.push('/link-device')}
            style={{ marginTop: spacing.base }}
          />
        </View>

        <Section
          title="Your devices"
          footer="If you see a device you do not recognise, sign it out and change your two-step PIN."
        >
          {devices === null ? (
            <SkeletonRow />
          ) : devices.length === 0 ? (
            <ListRow title="No other devices" subtitle="Only this phone is signed in" />
          ) : (
            devices.map((device) => (
              <ListRow
                key={device.id}
                icon={PLATFORM_ICONS[device.platform] ?? 'Smartphone'}
                title={device.device_name ?? platformLabel(device.platform)}
                subtitle={`${platformLabel(device.platform)} · ${lastActive(device.last_active_at)}`}
                onPress={device.is_primary ? undefined : () => confirmRevoke(device)}
                right={
                  device.is_primary ? (
                    <View style={[styles.chip, { backgroundColor: colors.primarySoft }]}>
                      <Text variant="caption" tone="primary">
                        This device
                      </Text>
                    </View>
                  ) : (
                    <Icon name="LogOut" size={17} color={colors.danger} />
                  )
                }
              />
            ))
          )}
        </Section>
      </View>
    </Screen>
  );
}

const styles = StyleSheet.create({
  hero: {
    alignItems: 'center',
    padding: spacing.xl,
    borderRadius: radius.lg,
    marginBottom: spacing.xl,
  },
  heroIcon: {
    width: 48,
    height: 48,
    borderRadius: 24,
    alignItems: 'center',
    justifyContent: 'center',
  },
  chip: { paddingHorizontal: 8, paddingVertical: 3, borderRadius: radius.pill },
});
