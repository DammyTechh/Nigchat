import { useRouter } from 'expo-router';
import React, { useState } from 'react';
import { Alert, KeyboardAvoidingView, Platform, StyleSheet, View } from 'react-native';

import { users as usersApi } from '../../src/api/endpoints';
import { Button, Header, Icon, Input, Screen, Text } from '../../src/components';
import { useAuth } from '../../src/store/auth';
import { radius, spacing, useColors } from '../../src/theme';

/**
 * Two-step verification (spec §14).
 *
 * This is the control that stops a SIM-swap attacker taking an account with a
 * hijacked SMS alone, so the copy says that plainly rather than calling it
 * "extra security".
 *
 * The rules below mirror what the server enforces — the client validates early
 * to save a round trip, but the server is the authority: 6–12 digits, no
 * repeats or runs, and the current PIN required to change or disable it.
 */
export default function TwoStepScreen() {
  const router = useRouter();
  const colors = useColors();
  const me = useAuth((state) => state.me);
  const refreshMe = useAuth((state) => state.refreshMe);

  const enabled = !!me?.two_step_enabled;

  const [currentPin, setCurrentPin] = useState('');
  const [pin, setPin] = useState('');
  const [confirmPin, setConfirmPin] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const localError = validate(pin, confirmPin);
  const ready = pin.length >= 6 && confirmPin.length >= 6 && !localError && (!enabled || currentPin.length >= 6);

  async function save() {
    setBusy(true);
    setError(null);
    try {
      await usersApi.setTwoStepPin(pin, enabled ? currentPin : undefined);
      await refreshMe();
      router.back();
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setBusy(false);
    }
  }

  function confirmDisable() {
    if (currentPin.length < 6) {
      setError('Enter your current PIN to turn this off.');
      return;
    }
    Alert.alert(
      'Turn off two-step verification?',
      'Anyone who takes over your phone number would then be able to sign in to your account with an SMS code alone.',
      [
        { text: 'Keep it on', style: 'cancel' },
        {
          text: 'Turn off',
          style: 'destructive',
          onPress: async () => {
            setBusy(true);
            try {
              await usersApi.disableTwoStep(currentPin);
              await refreshMe();
              router.back();
            } catch (err) {
              setError((err as Error).message);
            } finally {
              setBusy(false);
            }
          },
        },
      ],
    );
  }

  return (
    <Screen edges={['top', 'bottom']} scroll>
      <Header title={enabled ? 'Change your PIN' : 'Two-step verification'} back />

      <KeyboardAvoidingView behavior={Platform.OS === 'ios' ? 'padding' : undefined}>
        <View style={{ paddingHorizontal: spacing.base, paddingTop: spacing.base }}>
          <View style={[styles.hero, { backgroundColor: colors.primarySoft }]}>
            <Icon name="KeyRound" size={22} color={colors.primary} />
            <Text variant="footnote" style={{ flex: 1 }}>
              You&apos;ll be asked for this PIN when registering your number on a new
              device. It cannot be recovered by SMS, so choose something you will
              remember.
            </Text>
          </View>

          {enabled ? (
            <Input
              label="Current PIN"
              value={currentPin}
              onChangeText={(text) => setCurrentPin(text.replace(/\D/g, '').slice(0, 12))}
              placeholder="Your existing PIN"
              keyboardType="number-pad"
              secureTextEntry
              containerStyle={{ marginBottom: spacing.lg }}
            />
          ) : null}

          <Input
            label={enabled ? 'New PIN' : 'Create a PIN'}
            value={pin}
            onChangeText={(text) => setPin(text.replace(/\D/g, '').slice(0, 12))}
            placeholder="6 to 12 digits"
            keyboardType="number-pad"
            secureTextEntry
            autoFocus={!enabled}
            containerStyle={{ marginBottom: spacing.lg }}
          />

          <Input
            label="Confirm PIN"
            value={confirmPin}
            onChangeText={(text) => setConfirmPin(text.replace(/\D/g, '').slice(0, 12))}
            placeholder="Enter it again"
            keyboardType="number-pad"
            secureTextEntry
            error={error ?? (confirmPin.length >= 6 ? localError ?? undefined : undefined)}
          />

          <Button
            label={enabled ? 'Change PIN' : 'Turn on two-step verification'}
            fullWidth
            size="lg"
            loading={busy}
            disabled={!ready}
            onPress={save}
            style={{ marginTop: spacing.xl }}
          />

          {enabled ? (
            <Button
              label="Turn off two-step verification"
              variant="ghost"
              fullWidth
              onPress={confirmDisable}
              style={{ marginTop: spacing.sm }}
            />
          ) : null}

          <Text variant="caption" tone="muted" center style={{ marginTop: spacing.lg }}>
            After five wrong attempts you&apos;ll need to wait an hour before trying
            again.
          </Text>
        </View>
      </KeyboardAvoidingView>
    </Screen>
  );
}

/** Same rejections the server applies, checked here to save a round trip. */
function validate(pin: string, confirmPin: string): string | null {
  if (pin.length > 0 && pin.length < 6) return 'PIN must be at least 6 digits';
  if (confirmPin.length > 0 && pin !== confirmPin) return 'The two PINs do not match';

  const digits = [...pin].map(Number);
  if (pin.length >= 6) {
    const allSame = digits.every((digit) => digit === digits[0]);
    const ascending = digits.every((digit, index) => index === 0 || digit === digits[index - 1] + 1);
    const descending = digits.every((digit, index) => index === 0 || digit === digits[index - 1] - 1);
    if (allSame || ascending || descending) return 'Choose a less predictable PIN';
  }

  return null;
}

const styles = StyleSheet.create({
  hero: {
    flexDirection: 'row',
    gap: spacing.md,
    alignItems: 'flex-start',
    padding: spacing.base,
    borderRadius: radius.md,
    marginBottom: spacing.xl,
  },
});
