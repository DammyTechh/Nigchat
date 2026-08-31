import { create } from 'zustand';

export type ThemePreference = 'system' | 'light' | 'dark';

const KEY = 'nigchat.theme';

function systemPrefersDark() {
  return window.matchMedia('(prefers-color-scheme: dark)').matches;
}

function apply(preference: ThemePreference) {
  const dark = preference === 'dark' || (preference === 'system' && systemPrefersDark());
  document.documentElement.classList.toggle('dark', dark);
}

interface ThemeState {
  preference: ThemePreference;
  isDark: boolean;
  set: (preference: ThemePreference) => void;
}

const stored = (localStorage.getItem(KEY) as ThemePreference | null) ?? 'system';

export const useTheme = create<ThemeState>((set) => ({
  preference: stored,
  isDark: stored === 'dark' || (stored === 'system' && systemPrefersDark()),
  set(preference) {
    localStorage.setItem(KEY, preference);
    apply(preference);
    set({
      preference,
      isDark: preference === 'dark' || (preference === 'system' && systemPrefersDark()),
    });
  },
}));

// Following the OS means following it *live* — a user switching their system
// theme at sunset should see the app follow without a reload.
window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', () => {
  const { preference } = useTheme.getState();
  if (preference === 'system') {
    apply('system');
    useTheme.setState({ isDark: systemPrefersDark() });
  }
});

apply(stored);
