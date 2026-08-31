import Constants from 'expo-constants';
import * as Device from 'expo-device';
import * as Notifications from 'expo-notifications';
import { Platform } from 'react-native';

import { devices } from '../api/endpoints';

/**
 * Push registration.
 *
 * Called once after sign-in. Registration is best-effort: a user who declines
 * notifications, or a simulator with no push capability, must still get a fully
 * working app.
 */

// Foreground behaviour. The server already decided this notification was worth
// sending — mute, quiet hours and previews were all resolved before it left —
// so the client's job is only to present it.
Notifications.setNotificationHandler({
  handleNotification: async () => ({
    shouldShowAlert: true,
    shouldPlaySound: true,
    shouldSetBadge: true,
  }),
});

/**
 * Android channels carry the sound, not the message (API 26+). Each tone the
 * backend can select needs its own channel, created up front — a channel's
 * sound cannot be changed after creation, which is why switching tones means
 * switching channels rather than editing one.
 */
const ANDROID_CHANNELS = [
  { id: 'tone.message.default', name: 'Messages', sound: 'nigchat_message' },
  { id: 'tone.message.chime', name: 'Messages — Chime', sound: 'chime' },
  { id: 'tone.message.pulse', name: 'Messages — Pulse', sound: 'pulse' },
  { id: 'tone.message.drop', name: 'Messages — Drop', sound: 'drop' },
  { id: 'tone.message.silent', name: 'Messages — Silent', sound: null },
  { id: 'tone.group.default', name: 'Groups', sound: 'nigchat_group' },
  { id: 'tone.group.tap', name: 'Groups — Tap', sound: 'tap' },
  { id: 'tone.status.default', name: 'Status updates', sound: 'status_update' },
];

export async function registerForPush(): Promise<void> {
  // Push needs real hardware. Bailing early keeps the simulator log clean.
  if (!Device.isDevice) return;

  try {
    if (Platform.OS === 'android') {
      await Promise.all(
        ANDROID_CHANNELS.map((channel) =>
          Notifications.setNotificationChannelAsync(channel.id, {
            name: channel.name,
            importance: Notifications.AndroidImportance.HIGH,
            sound: channel.sound ?? undefined,
            vibrationPattern: [0, 200, 100, 200],
            lockscreenVisibility: Notifications.AndroidNotificationVisibility.PRIVATE,
          }),
        ),
      );

      // Calls ring rather than chime, and must break through Do Not Disturb.
      await Notifications.setNotificationChannelAsync('tone.call.default', {
        name: 'Calls',
        importance: Notifications.AndroidImportance.MAX,
        sound: 'nigchat_ring',
        vibrationPattern: [0, 1000, 500, 1000],
        bypassDnd: true,
      });

      // Security alerts are deliberately not user-silenceable on the server;
      // giving them their own MAX channel keeps that true on the device.
      await Notifications.setNotificationChannelAsync('tone.system.security', {
        name: 'Security alerts',
        importance: Notifications.AndroidImportance.MAX,
        sound: 'security_alert',
        bypassDnd: true,
      });
    }

    const existing = await Notifications.getPermissionsAsync();
    let granted = existing.granted;

    if (!granted && existing.canAskAgain) {
      const requested = await Notifications.requestPermissionsAsync();
      granted = requested.granted;
    }
    if (!granted) return;

    const projectId =
      Constants.expoConfig?.extra?.eas?.projectId ?? Constants.easConfig?.projectId;

    const token = await Notifications.getDevicePushTokenAsync();

    await devices.registerPushToken({
      provider: Platform.OS === 'ios' ? 'apns' : 'fcm',
      token: String(token.data),
      // Debug builds talk to the APNs sandbox; a production token sent to the
      // sandbox gateway is rejected, and vice versa.
      sandbox: Platform.OS === 'ios' && __DEV__,
    });

    void projectId;
  } catch {
    // Never surface this. A user without push still has a working messenger.
  }
}

/** Deep link target for a tapped notification. */
export function conversationIdFromNotification(
  response: Notifications.NotificationResponse,
): string | null {
  const data = response.notification.request.content.data as Record<string, unknown>;
  const payload = data?.payload;

  if (typeof payload === 'string') {
    try {
      return (JSON.parse(payload) as { conversation_id?: string }).conversation_id ?? null;
    } catch {
      return null;
    }
  }

  return (data?.conversation_id as string) ?? null;
}
