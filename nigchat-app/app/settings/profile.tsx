import { useRouter } from 'expo-router';
import React, { useEffect, useState } from 'react';
import { KeyboardAvoidingView, Platform, View } from 'react-native';

import { media as mediaApi, users as usersApi } from '../../src/api/endpoints';
import { Avatar, Button, Header, Icon, Input, Pressable, Screen, Text } from '../../src/components';
import { useAuth } from '../../src/store/auth';
import { spacing, useColors } from '../../src/theme';
import { prettyE164 } from '../../src/utils/phone';
import { pickAndUploadImage } from '../../src/utils/upload';

/**
 * Edit profile.
 *
 * The settings screen used to open nothing. This is the smallest real version:
 * the two fields the backend accepts today. A photo needs media upload, so the
 * avatar is a display until that exists rather than a button that fails.
 */
export default function EditProfileScreen() {
  const router = useRouter();
  const colors = useColors();
  const me = useAuth((state) => state.me);
  const refreshMe = useAuth((state) => state.refreshMe);

  const [avatarUrl, setAvatarUrl] = useState<string | null>(null);
  const [uploading, setUploading] = useState(false);

  const [displayName, setDisplayName] = useState('');
  const [about, setAbout] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setDisplayName(me?.display_name ?? '');
    setAbout(me?.about ?? '');

    // The profile carries a media id, not a URL — resolving it here keeps the
    // link fresh rather than caching one that can expire.
    if (me?.avatar_media_id) {
      mediaApi
        .get(me.avatar_media_id)
        .then((asset) => setAvatarUrl(asset.url))
        .catch(() => {});
    }
  }, [me]);

  async function changePhoto() {
    setError(null);
    setUploading(true);
    try {
      const uploaded = await pickAndUploadImage({ purpose: 'avatar', square: true });
      if (!uploaded) return; // cancelled

      // Show it immediately, then persist. The upload is the slow part and it
      // has already finished by now.
      setAvatarUrl(uploaded.url);
      await usersApi.updateMe({ avatar_media_id: uploaded.id });
      await refreshMe();
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setUploading(false);
    }
  }

  const trimmed = displayName.trim();
  const changed = trimmed !== (me?.display_name ?? '') || about !== (me?.about ?? '');

  async function save() {
    setBusy(true);
    setError(null);
    try {
      await usersApi.updateMe({ display_name: trimmed, about });
      await refreshMe();
      router.back();
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setBusy(false);
    }
  }

  return (
    <Screen edges={['top', 'bottom']} scroll>
      <Header title="Profile" back />

      <KeyboardAvoidingView behavior={Platform.OS === 'ios' ? 'padding' : undefined}>
        <View style={{ paddingHorizontal: spacing.base, paddingTop: spacing.base }}>
          <View style={{ alignItems: 'center', paddingVertical: spacing.lg }}>
            <Pressable
              onPress={changePhoto}
              highlight={false}
              accessibilityLabel="Change profile photo"
            >
              <Avatar name={trimmed || '?'} uri={avatarUrl} size="xl" />
              <View
                style={{
                  position: 'absolute',
                  right: -2,
                  bottom: -2,
                  width: 30,
                  height: 30,
                  borderRadius: 15,
                  borderWidth: 3,
                  alignItems: 'center',
                  justifyContent: 'center',
                  backgroundColor: colors.primary,
                  borderColor: colors.background,
                }}
              >
                <Icon name="Camera" size={14} color={colors.onPrimary} />
              </View>
            </Pressable>

            <Text variant="caption" tone="muted" style={{ marginTop: spacing.md }}>
              {uploading ? 'Uploading…' : 'Tap to change your photo'}
            </Text>
          </View>

          <Input
            label="Name"
            value={displayName}
            onChangeText={setDisplayName}
            placeholder="Your name"
            maxLength={64}
            autoCapitalize="words"
            containerStyle={{ marginBottom: spacing.lg }}
          />

          <Input
            label="About"
            value={about}
            onChangeText={setAbout}
            placeholder="Available"
            maxLength={140}
            multiline
            hint={`${about.length}/140`}
            error={error ?? undefined}
          />

          <View style={{ marginTop: spacing.xl }}>
            <Text variant="footnote" tone="muted">
              Phone number
            </Text>
            <Text variant="body" style={{ marginTop: 2 }}>
              {me?.phone_e164 ? prettyE164(me.phone_e164) : ''}
            </Text>
            <Text variant="caption" tone="muted" style={{ marginTop: spacing.xs }}>
              Changing your number is not supported yet.
            </Text>
          </View>

          <Button
            label="Save"
            fullWidth
            size="lg"
            loading={busy}
            disabled={!trimmed || !changed}
            onPress={save}
            style={{ marginTop: spacing.xxl }}
          />
        </View>
      </KeyboardAvoidingView>
    </Screen>
  );
}
