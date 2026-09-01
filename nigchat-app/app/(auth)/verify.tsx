import { useLocalSearchParams, useRouter } from 'expo-router';
import React, { useEffect, useRef, useState } from 'react';
import {
  KeyboardAvoidingView,
  Platform,
  StyleSheet,
  TextInput,
  View,
} from 'react-native';

import { Button, Header, Pressable, Screen, Text } from '../../src/components';
import { useAuth } from '../../src/store/auth';
import { radius, spacing, typography, useColors } from '../../src/theme';
import { prettyE164 } from '../../src/utils/phone';

const CODE_LENGTH = 6;
const RESEND_SECONDS = 60;

export default function VerifyScreen() {
  const router = useRouter();
  const colors = useColors();
  const { phone, debugCode } = useLocalSearchParams<{ phone: string; debugCode?: string }>();
  const { verifyOtp, requestOtp } = useAuth();

  const [code, setCode] = useState(debugCode ?? '');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [countdown, setCountdown] = useState(RESEND_SECONDS);
  const inputRef = useRef<TextInput>(null);

  useEffect(() => {
    const timer = setInterval(() => setCountdown((value) => Math.max(0, value - 1)), 1000);
    return () => clearInterval(timer);
  }, []);

  // Submit as soon as the sixth digit lands — one fewer tap on the most
  // repeated screen in onboarding.
  useEffect(() => {
    if (code.length === CODE_LENGTH && !busy) submit(code);
  }, [code]);

  async function submit(value: string) {
    setError(null);
    setBusy(true);
    try {
      const isNew = await verifyOtp(phone!, value);
      router.replace(isNew ? '/(auth)/profile' : '/(tabs)');
    } catch (err) {
      setError((err as Error).message);
      setCode('');
      inputRef.current?.focus();
    } finally {
      setBusy(false);
    }
  }

  async function resend() {
    if (countdown > 0) return;
    setCountdown(RESEND_SECONDS);
    setError(null);
    try {
      await requestOtp(phone!);
    } catch (err) {
      setError((err as Error).message);
    }
  }

  return (
    <Screen edges={['top', 'bottom']}>
      <Header back />
      <KeyboardAvoidingView
        style={{ flex: 1 }}
        behavior={Platform.OS === 'ios' ? 'padding' : undefined}
      >
        <View style={styles.body}>
          <Text variant="displayLarge">Enter the code</Text>
          <Text variant="callout" tone="muted" style={{ marginTop: spacing.sm }}>
            Sent to {prettyE164(phone ?? '')}.{' '}
            <Text variant="callout" tone="primary" onPress={() => router.back()}>
              Change
            </Text>
          </Text>

          {/* One hidden field behind six boxes. Six separate inputs break
              autofill, paste, and backspace in ways users notice immediately. */}
          <Pressable
            onPress={() => inputRef.current?.focus()}
            highlight={false}
            style={styles.boxes}
            accessibilityLabel="Verification code"
          >
            {Array.from({ length: CODE_LENGTH }).map((_, index) => {
              const char = code[index] ?? '';
              const active = index === code.length;
              return (
                <View
                  key={index}
                  style={[
                    styles.box,
                    {
                      backgroundColor: colors.surfaceRaised,
                      borderColor: error
                        ? colors.danger
                        : active
                          ? colors.primary
                          : colors.border,
                      borderWidth: active || error ? 1.5 : 1,
                    },
                  ]}
                >
                  <Text style={[typography.title, { color: colors.text }]}>{char}</Text>
                </View>
              );
            })}
          </Pressable>

          <TextInput
            ref={inputRef}
            value={code}
            onChangeText={(text) => setCode(text.replace(/\D/g, '').slice(0, CODE_LENGTH))}
            keyboardType="number-pad"
            // Pulls the code straight from the SMS on iOS and Android.
            textContentType="oneTimeCode"
            autoComplete="sms-otp"
            autoFocus
            maxLength={CODE_LENGTH}
            style={styles.hiddenInput}
          />

          {error ? (
            <Text variant="footnote" tone="danger" center style={{ marginTop: spacing.base }}>
              {error}
            </Text>
          ) : null}

          {debugCode ? (
            <Text variant="caption" tone="muted" center style={{ marginTop: spacing.base }}>
              Development mode — the code was filled in for you.
            </Text>
          ) : null}

          <Pressable onPress={resend} highlight={false} style={styles.resend}>
            <Text variant="subhead" tone={countdown > 0 ? 'muted' : 'primary'} center>
              {countdown > 0 ? `Resend code in ${countdown}s` : 'Resend code'}
            </Text>
          </Pressable>
        </View>

        <View style={styles.footer}>
          <Button
            label="Verify"
            fullWidth
            size="lg"
            loading={busy}
            disabled={code.length < CODE_LENGTH}
            onPress={() => submit(code)}
          />
        </View>
      </KeyboardAvoidingView>
    </Screen>
  );
}

const styles = StyleSheet.create({
  body: { flex: 1, paddingHorizontal: spacing.base, paddingTop: spacing.base },
  boxes: { flexDirection: 'row', gap: spacing.sm, marginTop: spacing.xxl },
  box: {
    flex: 1,
    aspectRatio: 0.82,
    maxWidth: 58,
    borderRadius: radius.md,
    alignItems: 'center',
    justifyContent: 'center',
  },
  hiddenInput: { position: 'absolute', opacity: 0, height: 1, width: 1 },
  resend: { marginTop: spacing.xl, paddingVertical: spacing.sm },
  footer: { padding: spacing.base },
});
