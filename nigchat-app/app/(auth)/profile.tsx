import { useRouter } from 'expo-router';
import React, { useState } from 'react';
import { KeyboardAvoidingView, Platform, StyleSheet, View } from 'react-native';

import { Avatar, Button, Input, Pressable, Screen, Text, Icon } from '../../src/components';
import { users } from '../../src/api/endpoints';
import { useAuth } from '../../src/store/auth';
import { spacing, useColors } from '../../src/theme';

export default function ProfileSetup() {
  const router = useRouter();
  const colors = useColors();
  const refreshMe = useAuth((state) => state.refreshMe);

  const [name, setName] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const trimmed = name.trim();

  async function save() {
    setBusy(true);
    setError(null);
    try {
      await users.updateMe({ display_name: trimmed });
      await refreshMe();
      router.replace('/(tabs)');
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setBusy(false);
    }
  }

  return (
    <Screen edges={['top', 'bottom']}>
      <KeyboardAvoidingView
        style={{ flex: 1 }}
        behavior={Platform.OS === 'ios' ? 'padding' : undefined}
      >
        <View style={styles.body}>
          <Text variant="displayLarge">Your profile</Text>
          <Text variant="callout" tone="muted" style={{ marginTop: spacing.sm }}>
            This is how you&apos;ll appear to people you message. You can change it later.
          </Text>

          <View style={styles.avatarBlock}>
            <Pressable onPress={() => {}} highlight={false} accessibilityLabel="Add a photo">
              <Avatar name={trimmed || '?'} size="xl" />
              <View style={[styles.cameraBadge, { backgroundColor: colors.primary, borderColor: colors.background }]}>
                <Icon name="Camera" size={14} color={colors.onPrimary} />
              </View>
            </Pressable>
            <Text variant="footnote" tone="muted" style={{ marginTop: spacing.md }}>
              Add a photo
            </Text>
          </View>

          <Input
            label="Display name"
            value={name}
            onChangeText={setName}
            placeholder="e.g. Ada Obi"
            maxLength={64}
            autoFocus
            autoCapitalize="words"
            returnKeyType="done"
            error={error ?? undefined}
            onSubmitEditing={trimmed ? save : undefined}
          />
        </View>

        <View style={styles.footer}>
          <Button
            label="Start messaging"
            fullWidth
            size="lg"
            loading={busy}
            disabled={trimmed.length === 0}
            onPress={save}
          />
        </View>
      </KeyboardAvoidingView>
    </Screen>
  );
}

const styles = StyleSheet.create({
  body: { flex: 1, paddingHorizontal: spacing.base, paddingTop: spacing.xxl },
  avatarBlock: { alignItems: 'center', paddingVertical: spacing.xxl },
  cameraBadge: {
    position: 'absolute',
    right: -2,
    bottom: -2,
    width: 30,
    height: 30,
    borderRadius: 15,
    borderWidth: 3,
    alignItems: 'center',
    justifyContent: 'center',
  },
  footer: { padding: spacing.base },
});
