/**
 * Design tokens.
 *
 * The greens are sampled directly from the logo artwork rather than guessed, so
 * the app and the mark are the same colour on screen:
 *   #0F663F  the deep side of the bubble gradient  -> `deep`
 *   #179759  the light side of the bubble          -> `primary`
 *   #22C55E  the bright "Chat" wordmark            -> `accent`
 *
 * Deliberate distance from the obvious competitor: green is an *accent* here,
 * never a surface. There is no green app bar, no green screen background. The
 * chrome is white or near-black; green appears only on things you can act on —
 * the send button, the unread count, the active tab, your own messages. That
 * one rule is most of what makes this read as a different product.
 */

export const palette = {
  // brand — sampled from assets/images/logo-full.png
  deep: '#0F663F',
  primary: '#0E7A46',
  primaryBright: '#179759',
  accent: '#22C55E',

  // neutrals, very slightly green-shifted so they sit under the brand without
  // looking like two unrelated palettes
  white: '#FFFFFF',
  grey50: '#F7F9F8',
  grey100: '#EFF3F1',
  grey200: '#E3E9E5',
  grey300: '#CBD5CF',
  grey400: '#9AA8A0',
  grey500: '#6B7A72',
  grey600: '#4A574F',
  grey700: '#333E38',
  grey800: '#1E2723',
  grey900: '#131A16',
  black: '#080C0A',

  // status
  red: '#DC2626',
  redDark: '#F87171',
  amber: '#D97706',
  amberDark: '#FBBF24',
  blue: '#2563EB',
  blueDark: '#60A5FA',
} as const;

export type ThemeName = 'light' | 'dark';

export interface Theme {
  name: ThemeName;
  colors: {
    /** Page background. */
    background: string;
    /** Cards, rows, grouped sections. */
    surface: string;
    /** One step further forward — input fields, incoming bubbles. */
    surfaceRaised: string;
    /** Pressed states. */
    surfacePressed: string;
    /** Hairlines. Never heavier than 1px at any density. */
    border: string;
    borderStrong: string;

    text: string;
    textSecondary: string;
    textMuted: string;
    /** For text sitting on `primary`. */
    onPrimary: string;

    primary: string;
    primaryPressed: string;
    /** Tinted background for the active tab pill, badges, selected rows. */
    primarySoft: string;
    accent: string;

    /** Your own messages. */
    bubbleOut: string;
    bubbleOutText: string;
    bubbleOutMeta: string;
    /** Everyone else's. */
    bubbleIn: string;
    bubbleInText: string;
    bubbleInMeta: string;
    bubbleInBorder: string;

    danger: string;
    warning: string;
    info: string;
    online: string;

    /** Behind sheets and dialogs. */
    scrim: string;
    /** Tab bar and headers, translucent over content. */
    chrome: string;
  };
}

export const lightTheme: Theme = {
  name: 'light',
  colors: {
    background: palette.white,
    surface: palette.white,
    surfaceRaised: palette.grey50,
    surfacePressed: palette.grey100,
    border: palette.grey200,
    borderStrong: palette.grey300,

    text: palette.grey900,
    textSecondary: palette.grey600,
    textMuted: palette.grey500,
    onPrimary: palette.white,

    primary: palette.primary,
    primaryPressed: palette.deep,
    primarySoft: '#E6F4EC',
    accent: palette.accent,

    bubbleOut: palette.primary,
    bubbleOutText: palette.white,
    bubbleOutMeta: 'rgba(255,255,255,0.72)',
    bubbleIn: palette.grey50,
    bubbleInText: palette.grey900,
    bubbleInMeta: palette.grey500,
    bubbleInBorder: palette.grey200,

    danger: palette.red,
    warning: palette.amber,
    info: palette.blue,
    online: palette.primaryBright,

    scrim: 'rgba(8,12,10,0.45)',
    chrome: 'rgba(255,255,255,0.92)',
  },
};

/**
 * Dark mode is a real design, not an inversion. Two things matter:
 *
 *  - The background is #0B120E, not pure black. On OLED, pure black behind a
 *    scrolling list produces visible smearing, and every elevation step above
 *    it has to be faked with borders.
 *  - The green is lifted to #179759. The light-mode primary fails contrast
 *    against a dark surface, and a brand colour that is unreadable at night is
 *    a bug, not a style.
 */
export const darkTheme: Theme = {
  name: 'dark',
  colors: {
    background: '#0B120E',
    surface: '#111A15',
    surfaceRaised: '#18231D',
    surfacePressed: '#1F2C25',
    border: '#22302A',
    borderStrong: '#2E3E36',

    text: '#E9F1EC',
    textSecondary: '#A8B8AF',
    textMuted: '#7C8C84',
    onPrimary: palette.white,

    primary: palette.primaryBright,
    primaryPressed: '#127A46',
    primarySoft: '#10301F',
    accent: palette.accent,

    bubbleOut: '#126E42',
    bubbleOutText: '#F2FBF5',
    bubbleOutMeta: 'rgba(242,251,245,0.62)',
    bubbleIn: '#18231D',
    bubbleInText: '#E9F1EC',
    bubbleInMeta: '#7C8C84',
    bubbleInBorder: '#22302A',

    danger: palette.redDark,
    warning: palette.amberDark,
    info: palette.blueDark,
    online: palette.accent,

    scrim: 'rgba(0,0,0,0.6)',
    chrome: 'rgba(11,18,14,0.92)',
  },
};

/**
 * 4pt grid. Every margin, padding and gap in the app comes from here — that
 * consistency is most of what separates a designed interface from an assembled
 * one.
 */
export const spacing = {
  xxs: 2,
  xs: 4,
  sm: 8,
  md: 12,
  base: 16,
  lg: 20,
  xl: 24,
  xxl: 32,
  xxxl: 40,
} as const;

export const radius = {
  sm: 8,
  md: 12,
  lg: 16,
  xl: 20,
  xxl: 28,
  pill: 999,
} as const;

/**
 * Shadows are used sparingly — only for things that genuinely float above the
 * page (the composer's send button, sheets, the FAB). Elevation everywhere else
 * is expressed with surface colour and hairlines, which stays crisp in dark
 * mode where shadows disappear.
 */
export const shadow = {
  none: {},
  subtle: {
    shadowColor: '#000',
    shadowOpacity: 0.06,
    shadowRadius: 8,
    shadowOffset: { width: 0, height: 2 },
    elevation: 2,
  },
  raised: {
    shadowColor: '#000',
    shadowOpacity: 0.12,
    shadowRadius: 20,
    shadowOffset: { width: 0, height: 8 },
    elevation: 8,
  },
} as const;

/** Layout constants shared across screens. */
export const layout = {
  /** Comfortable tap target. Below 44 fails accessibility on both platforms. */
  tapTarget: 44,
  avatar: { sm: 32, md: 40, lg: 52, xl: 88 },
  /** Content never stretches edge-to-edge on a tablet — long lines are hard to read. */
  maxContentWidth: 720,
  headerHeight: 52,
  tabBarHeight: 56,
} as const;
