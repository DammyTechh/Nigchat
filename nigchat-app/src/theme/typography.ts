import { Platform, TextStyle } from 'react-native';

/**
 * Type scale.
 *
 * System fonts on purpose: SF Pro on iOS, Roboto on Android. A custom face
 * would add a download, a flash of unstyled text, and would look subtly wrong
 * next to the platform's own keyboard and share sheet. Messaging apps live
 * inside the OS; they should read like it.
 */
const family = Platform.select({
  ios: 'System',
  android: 'sans-serif',
  default: 'System',
});

const familyMedium = Platform.select({
  ios: 'System',
  android: 'sans-serif-medium',
  default: 'System',
});

type Variant =
  | 'displayLarge'
  | 'title'
  | 'titleSmall'
  | 'headline'
  | 'body'
  | 'bodyStrong'
  | 'callout'
  | 'subhead'
  | 'footnote'
  | 'caption'
  | 'overline';

export const typography: Record<Variant, TextStyle> = {
  /** Screen titles in the large-title header. */
  displayLarge: {
    fontFamily: familyMedium,
    fontSize: 32,
    lineHeight: 38,
    fontWeight: '700',
    letterSpacing: -0.6,
  },
  /** Section and sheet titles. */
  title: {
    fontFamily: familyMedium,
    fontSize: 22,
    lineHeight: 28,
    fontWeight: '700',
    letterSpacing: -0.3,
  },
  titleSmall: {
    fontFamily: familyMedium,
    fontSize: 17,
    lineHeight: 22,
    fontWeight: '600',
    letterSpacing: -0.2,
  },
  /** Conversation names in a list row. */
  headline: {
    fontFamily: familyMedium,
    fontSize: 16,
    lineHeight: 21,
    fontWeight: '600',
    letterSpacing: -0.1,
  },
  /** Message text and general copy. */
  body: {
    fontFamily: family,
    fontSize: 16,
    lineHeight: 22,
    fontWeight: '400',
  },
  bodyStrong: {
    fontFamily: familyMedium,
    fontSize: 16,
    lineHeight: 22,
    fontWeight: '600',
  },
  /** Message previews in a row. */
  callout: {
    fontFamily: family,
    fontSize: 15,
    lineHeight: 20,
    fontWeight: '400',
  },
  subhead: {
    fontFamily: family,
    fontSize: 14,
    lineHeight: 19,
    fontWeight: '400',
  },
  /** Timestamps, helper text. */
  footnote: {
    fontFamily: family,
    fontSize: 13,
    lineHeight: 17,
    fontWeight: '400',
  },
  /** Bubble metadata, badge counts. */
  caption: {
    fontFamily: family,
    fontSize: 11,
    lineHeight: 14,
    fontWeight: '500',
  },
  /** Grouped-list section headers. */
  overline: {
    fontFamily: familyMedium,
    fontSize: 12,
    lineHeight: 16,
    fontWeight: '600',
    letterSpacing: 0.6,
    textTransform: 'uppercase',
  },
};
