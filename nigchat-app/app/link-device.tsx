import { CameraView, useCameraPermissions } from 'expo-camera';
import { useRouter } from 'expo-router';
import React, { useRef, useState } from 'react';
import { StyleSheet, useWindowDimensions, View } from 'react-native';

import { deviceLinks } from '../src/api/endpoints';
import { Button, Glass, Header, Icon, Screen, Text } from '../src/components';
import { radius, spacing, useColors } from '../src/theme';

/**
 * QR pairing.
 *
 * The web client shows a code; this phone scans it and authorises the session.
 * The phone is the root of trust — the browser never receives the account's
 * long-term keys, which is why pairing has to happen this way round rather than
 * by typing a password into a website.
 */
export default function LinkDeviceScreen() {
  const router = useRouter();
  const colors = useColors();
  const { width } = useWindowDimensions();
  const [permission, requestPermission] = useCameraPermissions();
  const [state, setState] = useState<'scanning' | 'confirming' | 'linking' | 'done'>(
    'scanning',
  );
  const [code, setCode] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const handled = useRef(false);

  // The reticle scales with the screen so it looks right on an SE and an iPad.
  const frameSize = Math.min(width * 0.68, 300);

  function onScanned({ data }: { data: string }) {
    // The camera fires continuously; without this guard one code would trigger
    // the request a dozen times.
    if (handled.current) return;
    handled.current = true;

    setCode(data.trim());
    // Scanning is not consent. The user sees what they are authorising and
    // confirms it — a QR pointed at a phone should never link an account on
    // its own.
    setState('confirming');
  }

  async function confirmLink() {
    if (!code) return;
    setState('linking');
    setError(null);

    try {
      await deviceLinks.approve(code);
      setState('done');
      // A beat so the browser's next poll picks up the approval before this
      // screen disappears.
      setTimeout(() => router.back(), 900);
    } catch (err) {
      setError((err as Error).message);
      setState('confirming');
      handled.current = false;
    }
  }

  if (!permission) {
    return (
      <Screen edges={['top', 'bottom']}>
        <Header title="Link a device" back />
      </Screen>
    );
  }

  if (!permission.granted) {
    return (
      <Screen edges={['top', 'bottom']}>
        <Header title="Link a device" back />
        <View style={styles.permission}>
          <View style={[styles.permissionIcon, { backgroundColor: colors.primarySoft }]}>
            <Icon name="Camera" size={26} color={colors.primary} strokeWidth={1.8} />
          </View>
          <Text variant="titleSmall" center style={{ marginTop: spacing.base }}>
            Camera access needed
          </Text>
          <Text variant="subhead" tone="muted" center style={{ marginTop: spacing.xs, maxWidth: 300 }}>
            NigChat needs the camera to read the pairing code on your computer screen. It is
            used for nothing else.
          </Text>
          <Button
            label="Allow camera"
            onPress={requestPermission}
            style={{ marginTop: spacing.lg }}
          />
        </View>
      </Screen>
    );
  }

  if (state !== 'scanning') {
    const linked = state === 'done';

    return (
      <Screen edges={['top', 'bottom']}>
        <Header title="Link a device" back />
        <View style={styles.permission}>
          <View style={[styles.permissionIcon, { backgroundColor: colors.primarySoft }]}>
            <Icon
              name={linked ? 'CircleCheck' : 'MonitorSmartphone'}
              size={26}
              color={colors.primary}
              strokeWidth={1.8}
            />
          </View>

          <Text variant="titleSmall" center style={{ marginTop: spacing.base }}>
            {linked ? 'Device linked' : 'Link this computer?'}
          </Text>

          <Text
            variant="subhead"
            tone="muted"
            center
            style={{ marginTop: spacing.xs, maxWidth: 320 }}
          >
            {linked
              ? 'Your chats are loading on the other screen.'
              : 'It will be able to read and send messages from your account until you sign it out. Only continue if the code is on a screen in front of you.'}
          </Text>

          {error ? (
            <Text variant="footnote" tone="danger" center style={{ marginTop: spacing.base }}>
              {error}
            </Text>
          ) : null}

          {!linked && (
            <>
              <Button
                label="Link device"
                fullWidth
                size="lg"
                loading={state === 'linking'}
                onPress={confirmLink}
                style={{ marginTop: spacing.xl }}
              />
              <Button
                label="Cancel"
                variant="ghost"
                fullWidth
                onPress={() => router.back()}
                style={{ marginTop: spacing.sm }}
              />
            </>
          )}
        </View>
      </Screen>
    );
  }

  return (
    <View style={{ flex: 1, backgroundColor: '#000' }}>
      <CameraView
        style={StyleSheet.absoluteFill}
        facing="back"
        barcodeScannerSettings={{ barcodeTypes: ['qr'] }}
        onBarcodeScanned={onScanned}
      />

      <Screen edges={['top', 'bottom']} style={{ backgroundColor: 'transparent' }}>
        <Header
          title="Scan to link"
          back
          borderless
          style={{ backgroundColor: 'transparent' }}
        />

        <View style={styles.overlay}>
          <View style={[styles.frame, { width: frameSize, height: frameSize }]}>
            {/* Corner brackets rather than a full border: they read as a
                viewfinder and leave the code unobscured. */}
            {(['tl', 'tr', 'bl', 'br'] as const).map((corner) => (
              <View key={corner} style={[styles.corner, cornerStyles[corner], { borderColor: colors.accent }]} />
            ))}
          </View>

          <Glass elevation="overlay" style={styles.hint}>
            <Text variant="subhead" center>
              Open nigchat.com on your computer and point your camera at the code.
            </Text>
          </Glass>
        </View>
      </Screen>
    </View>
  );
}

const BRACKET = 34;
const THICKNESS = 3;

const cornerStyles = StyleSheet.create({
  tl: { top: -1, left: -1, borderTopWidth: THICKNESS, borderLeftWidth: THICKNESS, borderTopLeftRadius: radius.lg },
  tr: { top: -1, right: -1, borderTopWidth: THICKNESS, borderRightWidth: THICKNESS, borderTopRightRadius: radius.lg },
  bl: { bottom: -1, left: -1, borderBottomWidth: THICKNESS, borderLeftWidth: THICKNESS, borderBottomLeftRadius: radius.lg },
  br: { bottom: -1, right: -1, borderBottomWidth: THICKNESS, borderRightWidth: THICKNESS, borderBottomRightRadius: radius.lg },
});

const styles = StyleSheet.create({
  overlay: { flex: 1, alignItems: 'center', justifyContent: 'center' },
  hint: {
    marginTop: spacing.xl,
    maxWidth: 320,
    paddingHorizontal: spacing.lg,
    paddingVertical: spacing.base,
    borderRadius: radius.lg,
    overflow: 'hidden',
  },
  frame: { position: 'relative' },
  corner: { position: 'absolute', width: BRACKET, height: BRACKET },
  permission: { flex: 1, alignItems: 'center', justifyContent: 'center', padding: spacing.xl },
  permissionIcon: {
    width: 64,
    height: 64,
    borderRadius: 32,
    alignItems: 'center',
    justifyContent: 'center',
  },
});
