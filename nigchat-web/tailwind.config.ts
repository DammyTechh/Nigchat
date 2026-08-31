import type { Config } from 'tailwindcss';

/**
 * The same design tokens as the mobile app, expressed as CSS variables so both
 * themes come from one source. Colours are written as `rgb(var(--x) / <alpha>)`
 * so Tailwind's opacity modifiers (`bg-surface/60`) still work — which the
 * glass surfaces depend on.
 */
export default {
  content: ['./index.html', './src/**/*.{ts,tsx}'],
  darkMode: 'class',
  theme: {
    extend: {
      colors: {
        bg: 'rgb(var(--bg) / <alpha-value>)',
        surface: 'rgb(var(--surface) / <alpha-value>)',
        raised: 'rgb(var(--raised) / <alpha-value>)',
        pressed: 'rgb(var(--pressed) / <alpha-value>)',
        line: 'rgb(var(--line) / <alpha-value>)',
        'line-strong': 'rgb(var(--line-strong) / <alpha-value>)',
        ink: 'rgb(var(--ink) / <alpha-value>)',
        'ink-2': 'rgb(var(--ink-2) / <alpha-value>)',
        'ink-3': 'rgb(var(--ink-3) / <alpha-value>)',
        brand: 'rgb(var(--brand) / <alpha-value>)',
        'brand-deep': 'rgb(var(--brand-deep) / <alpha-value>)',
        'brand-soft': 'rgb(var(--brand-soft) / <alpha-value>)',
        accent: 'rgb(var(--accent) / <alpha-value>)',
        'bubble-out': 'rgb(var(--bubble-out) / <alpha-value>)',
        'bubble-in': 'rgb(var(--bubble-in) / <alpha-value>)',
        danger: 'rgb(var(--danger) / <alpha-value>)',
        warning: 'rgb(var(--warning) / <alpha-value>)',
      },
      borderRadius: {
        bubble: '20px',
        'bubble-tail': '6px',
      },
      fontFamily: {
        sans: [
          // The platform's own UI face. A messaging client should read like
          // part of the OS, not like a marketing site.
          '-apple-system',
          'BlinkMacSystemFont',
          '"Segoe UI"',
          'Roboto',
          '"Helvetica Neue"',
          'Arial',
          'sans-serif',
        ],
      },
      fontSize: {
        // Matches the app's type scale exactly.
        caption: ['11px', { lineHeight: '14px' }],
        footnote: ['13px', { lineHeight: '17px' }],
        subhead: ['14px', { lineHeight: '19px' }],
        callout: ['15px', { lineHeight: '20px' }],
        body: ['16px', { lineHeight: '22px' }],
        headline: ['16px', { lineHeight: '21px', fontWeight: '600' }],
        title: ['22px', { lineHeight: '28px', letterSpacing: '-0.3px' }],
        display: ['32px', { lineHeight: '38px', letterSpacing: '-0.6px' }],
      },
      boxShadow: {
        subtle: '0 2px 8px rgb(0 0 0 / 0.06)',
        raised: '0 8px 28px rgb(0 0 0 / 0.12)',
      },
      keyframes: {
        'fade-up': {
          from: { opacity: '0', transform: 'translateY(6px)' },
          to: { opacity: '1', transform: 'translateY(0)' },
        },
      },
      animation: {
        'fade-up': 'fade-up 180ms cubic-bezier(0.2, 0.8, 0.2, 1)',
      },
    },
  },
  plugins: [],
} satisfies Config;
