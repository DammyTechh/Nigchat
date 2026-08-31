import * as Application from 'expo-constants';
import Constants from 'expo-constants';
import * as Device from 'expo-device';
import { Platform } from 'react-native';
import { create } from 'zustand';

import { auth as authApi, users as usersApi } from '../api/endpoints';
import { setUnauthorizedHandler, tokens } from '../api/client';
import { socket } from '../api/socket';
import type { Me } from '../api/types';

type Status = 'loading' | 'signed-out' | 'signed-in';

interface AuthState {
  status: Status;
  me: Me | null;
  userId: string | null;
  deviceId: string | null;
  restore: () => Promise<void>;
  requestOtp: (phone: string) => Promise<{ expiresIn: number; debugCode?: string }>;
  verifyOtp: (phone: string, code: string, displayName?: string) => Promise<boolean>;
  refreshMe: () => Promise<void>;
  signOut: () => Promise<void>;
}

/** The backend's platform enum, resolved from the running device. */
function platformName(): string {
  if (Platform.OS === 'ios') {
    // iPadOS reports as 'ios'; the device type distinguishes them, and the
    // backend tracks them separately so a tablet can be identified in the
    // user's linked-devices list.
    return Device.deviceType === Device.DeviceType.TABLET ? 'ipados' : 'ios';
  }
  if (Platform.OS === 'android') {
    return Device.deviceType === Device.DeviceType.TABLET ? 'android_tablet' : 'android';
  }
  return 'web';
}

export const useAuth = create<AuthState>((set, get) => ({
  status: 'loading',
  me: null,
  userId: null,
  deviceId: null,

  async restore() {
    const stored = await tokens.get();
    if (!stored.access || !stored.refresh) {
      set({ status: 'signed-out' });
      return;
    }

    set({ userId: stored.userId, deviceId: stored.deviceId, status: 'signed-in' });

    // Optimistically signed in, then confirmed. Blocking the UI on a network
    // round trip at launch is the difference between an app that opens
    // instantly and one that shows a spinner on a bad connection.
    socket.connect();
    get().refreshMe().catch(() => {});
  },

  async requestOtp(phone) {
    const result = await authApi.requestOtp(phone);
    return { expiresIn: result.expires_in, debugCode: result.debug_code };
  },

  async verifyOtp(phone, code, displayName) {
    const { deviceId } = await tokens.get();

    const pair = await authApi.verifyOtp({
      phone_e164: phone,
      code,
      display_name: displayName,
      platform: platformName(),
      device_name: Device.deviceName ?? Device.modelName ?? undefined,
      app_version: Constants.expoConfig?.version ?? '1.0.0',
      // Reusing a known device id keeps reinstalls from piling up entries in
      // the linked-devices list.
      device_id: deviceId ?? undefined,
    });

    await tokens.save(pair);
    set({ status: 'signed-in', userId: pair.user_id, deviceId: pair.device_id });

    socket.connect();
    get().refreshMe().catch(() => {});

    return pair.is_new_account;
  },

  async refreshMe() {
    const me = await usersApi.me();
    set({ me });
  },

  async signOut() {
    // Best effort: if the network is down the local session still ends.
    await authApi.logout().catch(() => {});
    await tokens.clear();
    socket.disconnect();
    set({ status: 'signed-out', me: null, userId: null });
  },
}));

// A refresh token that the server rejects means the session is genuinely over —
// revoked from another device, or expired. Drop straight to the sign-in screen
// rather than leaving the user tapping a dead UI.
setUnauthorizedHandler(() => {
  useAuth.setState({ status: 'signed-out', me: null, userId: null });
  socket.disconnect();
});
