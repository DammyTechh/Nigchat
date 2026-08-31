import * as LocalAuthentication from 'expo-local-authentication';
import * as SecureStore from 'expo-secure-store';
import { Platform } from 'react-native';

/**
 * Biometric device lock.
 *
 * What this is: a **local gate** on this device. It stops someone holding your
 * unlocked phone from reading your messages.
 *
 * What it is not — and the UI must never imply otherwise:
 *   * It is not authentication to the server. The session token is what the
 *     backend trusts; Face ID never reaches it.
 *   * It is not encryption. Messages are already end-to-end encrypted; this
 *     does not make them "more" encrypted.
 *   * It cannot replace the SMS code or the two-step PIN when registering on a
 *     new device, because biometrics do not transfer between devices.
 *
 * Conflating these is how apps end up claiming security properties they do not
 * have.
 */

const ENABLED_KEY = 'nigchat.app-lock';
const LOCKED_CHATS_KEY = 'nigchat.locked-chats';

export type BiometricKind = 'face' | 'fingerprint' | 'iris' | 'passcode' | 'none';

export interface BiometricCapability {
  /** Hardware exists and at least one biometric is enrolled. */
  available: boolean;
  kind: BiometricKind;
  /** Human label for settings copy: "Face ID", "Fingerprint", … */
  label: string;
}

/**
 * Reads whatever the user has actually set up on this phone, rather than
 * assuming Face ID.
 *
 * The important case is Android. `isEnrolledAsync()` alone reports *biometrics*
 * only, so a user whose lock screen is a pattern or a PIN — extremely common,
 * and on plenty of devices the only option — would be told the feature is
 * unavailable. `getEnrolledLevelAsync()` distinguishes:
 *
 *   NONE       no lock screen at all      -> genuinely unavailable
 *   SECRET     pattern / PIN / password   -> usable, prompt falls back to it
 *   BIOMETRIC  fingerprint / face / iris  -> usable, prompt uses the sensor
 *
 * Whatever the user chose for their phone is what NigChat asks for. We never
 * insist on a specific method.
 */
export async function getCapability(): Promise<BiometricCapability> {
  const [hasHardware, level, types] = await Promise.all([
    LocalAuthentication.hasHardwareAsync(),
    LocalAuthentication.getEnrolledLevelAsync(),
    LocalAuthentication.supportedAuthenticationTypesAsync(),
  ]);

  const { NONE, SECRET } = LocalAuthentication.SecurityLevel;

  // No screen lock configured. Nothing to authenticate against, so offering
  // the toggle would only produce a prompt that always fails.
  if (level === NONE) {
    return { available: false, kind: 'none', label: 'No screen lock set' };
  }

  // A pattern, PIN or password but no enrolled biometric — either because the
  // phone has no sensor, or because the user chose not to use it.
  if (level === SECRET || !hasHardware) {
    return {
      available: true,
      kind: 'passcode',
      label: Platform.OS === 'ios' ? 'passcode' : 'screen lock',
    };
  }

  const { FACIAL_RECOGNITION, FINGERPRINT, IRIS } = LocalAuthentication.AuthenticationType;

  // Order matters: a phone can report several. Face first on iOS because a
  // Face ID device never also has Touch ID; on Android, fingerprint is the
  // more reliable sensor where both exist.
  const preference =
    Platform.OS === 'ios'
      ? [FACIAL_RECOGNITION, FINGERPRINT, IRIS]
      : [FINGERPRINT, FACIAL_RECOGNITION, IRIS];

  for (const type of preference) {
    if (!types.includes(type)) continue;

    if (type === FACIAL_RECOGNITION) {
      return {
        available: true,
        kind: 'face',
        // "Face ID" is Apple's brand and must not be used on Android.
        label: Platform.OS === 'ios' ? 'Face ID' : 'face unlock',
      };
    }
    if (type === FINGERPRINT) {
      return {
        available: true,
        kind: 'fingerprint',
        label: Platform.OS === 'ios' ? 'Touch ID' : 'fingerprint',
      };
    }
    return { available: true, kind: 'iris', label: 'iris unlock' };
  }

  return { available: true, kind: 'passcode', label: 'screen lock' };
}

/**
 * Prompts using whatever the phone is set up for.
 *
 * `disableDeviceFallback: false` lets the pattern, PIN or password stand in
 * when a scan fails — a mask, a wet finger, a cracked sensor. Without that
 * fallback a scratch on the reader locks someone out of their own messages.
 */
export async function authenticate(reason: string): Promise<boolean> {
  const capability = await getCapability();
  if (!capability.available) return true; // nothing to check against

  const result = await LocalAuthentication.authenticateAsync({
    promptMessage: reason,
    cancelLabel: 'Cancel',
    fallbackLabel: 'Use passcode',
    disableDeviceFallback: false,
  });

  return result.success;
}

export const appLock = {
  async isEnabled(): Promise<boolean> {
    return (await SecureStore.getItemAsync(ENABLED_KEY)) === 'true';
  },

  /**
   * Turning the lock **on** requires a successful scan first. Otherwise someone
   * holding an unlocked phone could enable it against a face that is not the
   * owner's, and the owner would be the one locked out.
   */
  async enable(): Promise<boolean> {
    const ok = await authenticate('Confirm it\u2019s you to turn on app lock');
    if (!ok) return false;
    await SecureStore.setItemAsync(ENABLED_KEY, 'true');
    return true;
  },

  /** Turning it **off** also requires a scan — otherwise the lock is theatre. */
  async disable(): Promise<boolean> {
    const ok = await authenticate('Confirm it\u2019s you to turn off app lock');
    if (!ok) return false;
    await SecureStore.deleteItemAsync(ENABLED_KEY);
    return true;
  },
};

export const lockedChats = {
  async list(): Promise<string[]> {
    const raw = await SecureStore.getItemAsync(LOCKED_CHATS_KEY);
    try {
      return raw ? (JSON.parse(raw) as string[]) : [];
    } catch {
      return [];
    }
  },

  async isLocked(conversationId: string): Promise<boolean> {
    return (await lockedChats.list()).includes(conversationId);
  },

  async toggle(conversationId: string): Promise<boolean> {
    const ok = await authenticate('Confirm it\u2019s you to change chat lock');
    if (!ok) return false;

    const current = await lockedChats.list();
    const next = current.includes(conversationId)
      ? current.filter((id) => id !== conversationId)
      : [...current, conversationId];

    await SecureStore.setItemAsync(LOCKED_CHATS_KEY, JSON.stringify(next));
    return true;
  },
};
