import { Image } from 'expo-image';
import React, { useCallback, useEffect, useRef, useState } from 'react';
import { AppState, AppStateStatus, StyleSheet, View } from 'react-native';

import { appLock, authenticate, getCapability } from '../utils/biometrics';
import { spacing, useColors } from '../theme';
import { Button } from './Button';
import { Text } from './Text';

/**
 * App lock.
 *
 * Wraps the whole tree. Two behaviours matter and both are easy to get wrong:
 *
 * **1. Re-lock on background, not on every foreground event.** iOS fires
 * `inactive` for a notification banner, the app switcher, a permission dialog —
 * and the biometric prompt itself. Re-authenticating on those means the prompt
 * triggers the state change that re-triggers the prompt. Only a real
 * `background` transition arms the lock.
 *
 * **2. A grace period.** Returning within a few seconds — switching to the
 * camera to attach a photo, tapping a link and coming back — should not demand
 * a face scan. Long enough to be usable, short enough that a phone left on a
 * desk locks.
 */

const GRACE_MS = 15_000;

export function LockGate({ children }: { children: React.ReactNode }) {
  const colors = useColors();

  const [enabled, setEnabled] = useState<boolean | null>(null);
  const [locked, setLocked] = useState(false);
  const [label, setLabel] = useState('your screen lock');
  const [prompting, setPrompting] = useState(false);

  const backgroundedAt = useRef<number | null>(null);
  const appStateRef = useRef<AppStateStatus>(AppState.currentState);

  useEffect(() => {
    (async () => {
      const on = await appLock.isEnabled();
      setEnabled(on);
      setLocked(on);
      setLabel((await getCapability()).label);
    })();
  }, []);

  const unlock = useCallback(async () => {
    if (prompting) return;
    setPrompting(true);
    try {
      const ok = await authenticate('Unlock NigChat');
      if (ok) setLocked(false);
    } finally {
      setPrompting(false);
    }
  }, [prompting]);

  // Prompt as soon as the lock screen appears, so the common path is a glance
  // rather than a tap then a glance.
  useEffect(() => {
    if (locked && enabled) unlock();
  }, [locked, enabled]);

  useEffect(() => {
    const subscription = AppState.addEventListener('change', (next) => {
      const previous = appStateRef.current;
      appStateRef.current = next;

      if (!enabled) return;

      if (next === 'background') {
        backgroundedAt.current = Date.now();
        return;
      }

      if (previous === 'background' && next === 'active') {
        const away = Date.now() - (backgroundedAt.current ?? 0);
        if (away > GRACE_MS) setLocked(true);
      }
    });

    return () => subscription.remove();
  }, [enabled]);

  // Still reading the setting: render nothing rather than flashing the app for
  // a frame before the lock appears.
  if (enabled === null) return null;
  if (!enabled || !locked) return <>{children}</>;

  return (
    <View style={[styles.root, { backgroundColor: colors.background }]}>
      <Image
        source={require('../../assets/images/logo-mark.png')}
        style={styles.logo}
        contentFit="contain"
      />
      <Text variant="titleSmall" center style={{ marginTop: spacing.lg }}>
        NigChat is locked
      </Text>
      <Text variant="subhead" tone="muted" center style={{ marginTop: spacing.xs, maxWidth: 280 }}>
        Unlock with {label} to see your messages.
      </Text>
      <Button
        label={`Unlock with ${label}`}
        icon="Fingerprint"
        onPress={unlock}
        loading={prompting}
        style={{ marginTop: spacing.xl }}
      />
    </View>
  );
}

const styles = StyleSheet.create({
  root: { flex: 1, alignItems: 'center', justifyContent: 'center', padding: spacing.xl },
  logo: { width: 72, height: 70 },
});
