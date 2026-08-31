import { useRouter } from 'expo-router';
import React, { useMemo, useState } from 'react';
import { KeyboardAvoidingView, Platform, ScrollView, StyleSheet, View } from 'react-native';

import { Button, Header, Icon, Input, Pressable, Screen, Text } from '../../src/components';
import { useAuth } from '../../src/store/auth';
import { radius, spacing, useColors } from '../../src/theme';
import {
  COUNTRIES,
  Country,
  DEFAULT_COUNTRY,
  formatAsYouType,
  isPlausible,
  toE164,
} from '../../src/utils/phone';

export default function PhoneScreen() {
  const router = useRouter();
  const colors = useColors();
  const requestOtp = useAuth((state) => state.requestOtp);

  const [country, setCountry] = useState<Country>(DEFAULT_COUNTRY);
  const [national, setNational] = useState('');
  const [picking, setPicking] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const ready = useMemo(() => isPlausible(country, national), [country, national]);

  async function submit() {
    setError(null);
    setBusy(true);
    try {
      const e164 = toE164(country, national);
      const result = await requestOtp(e164);
      router.push({
        pathname: '/(auth)/verify',
        params: { phone: e164, debugCode: result.debugCode ?? '' },
      });
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setBusy(false);
    }
  }

  if (picking) {
    return (
      <Screen edges={['top', 'bottom']}>
        <Header title="Select country" back onBack={() => setPicking(false)} />
        <ScrollView contentContainerStyle={{ paddingBottom: spacing.xxl }}>
          {COUNTRIES.map((item) => (
            <Pressable
              key={item.code + item.dial}
              onPress={() => {
                setCountry(item);
                setPicking(false);
              }}
              style={styles.countryRow}
            >
              <Text style={{ fontSize: 22 }}>{item.flag}</Text>
              <Text variant="body" style={{ flex: 1 }}>
                {item.name}
              </Text>
              <Text variant="body" tone="muted">
                {item.dial}
              </Text>
              {item.code === country.code ? (
                <Icon name="Check" size={18} color={colors.primary} />
              ) : null}
            </Pressable>
          ))}
        </ScrollView>
      </Screen>
    );
  }

  return (
    <Screen edges={['top', 'bottom']}>
      <Header back />
      <KeyboardAvoidingView
        style={{ flex: 1 }}
        behavior={Platform.OS === 'ios' ? 'padding' : undefined}
      >
        <View style={styles.body}>
          <Text variant="displayLarge">Your number</Text>
          <Text variant="callout" tone="muted" style={{ marginTop: spacing.sm }}>
            We&apos;ll text you a six-digit code to confirm it&apos;s you. Standard message rates
            may apply.
          </Text>

          <View style={styles.field}>
            <Pressable
              onPress={() => setPicking(true)}
              accessibilityLabel="Select country dialling code"
              style={[
                styles.dial,
                { backgroundColor: colors.surfaceRaised, borderColor: colors.border },
              ]}
            >
              <Text style={{ fontSize: 18 }}>{country.flag}</Text>
              <Text variant="body">{country.dial}</Text>
              <Icon name="ChevronDown" size={15} color={colors.textMuted} />
            </Pressable>

            <Input
              containerStyle={{ flex: 1 }}
              value={formatAsYouType(national)}
              onChangeText={(text) => setNational(text.replace(/\D/g, ''))}
              placeholder="801 234 5678"
              keyboardType="phone-pad"
              // Lets iOS fill the code from the incoming SMS on the next screen.
              textContentType="telephoneNumber"
              autoFocus
              maxLength={18}
              returnKeyType="done"
              onSubmitEditing={ready ? submit : undefined}
            />
          </View>

          {error ? (
            <Text variant="footnote" tone="danger" style={{ marginTop: spacing.md }}>
              {error}
            </Text>
          ) : null}
        </View>

        <View style={styles.footer}>
          <Button
            label="Continue"
            fullWidth
            size="lg"
            loading={busy}
            disabled={!ready}
            onPress={submit}
          />
        </View>
      </KeyboardAvoidingView>
    </Screen>
  );
}

const styles = StyleSheet.create({
  body: { flex: 1, paddingHorizontal: spacing.base, paddingTop: spacing.base },
  field: { flexDirection: 'row', gap: spacing.sm, marginTop: spacing.xl, alignItems: 'flex-start' },
  dial: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 6,
    height: 50,
    paddingHorizontal: spacing.md,
    borderRadius: radius.md,
    borderWidth: 1,
  },
  footer: { padding: spacing.base },
  countryRow: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: spacing.base,
    paddingHorizontal: spacing.base,
    paddingVertical: 14,
  },
});
