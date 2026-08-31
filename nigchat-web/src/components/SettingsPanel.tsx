import { Bell, Laptop, Monitor, Moon, ShieldCheck, Smartphone, Sun, Tablet } from 'lucide-react';
import { useEffect, useState } from 'react';

import { devices as devicesApi } from '../lib/endpoints';
import { useTheme, type ThemePreference } from '../lib/theme';
import type { Device } from '../lib/types';
import { Spinner } from './primitives';

const PLATFORM_ICON: Record<string, typeof Smartphone> = {
  ios: Smartphone,
  android: Smartphone,
  ipados: Tablet,
  android_tablet: Tablet,
  web: Monitor,
  windows: Monitor,
  macos: Laptop,
  linux: Monitor,
};

const THEMES: { value: ThemePreference; label: string; icon: typeof Sun }[] = [
  { value: 'system', label: 'System', icon: Monitor },
  { value: 'light', label: 'Light', icon: Sun },
  { value: 'dark', label: 'Dark', icon: Moon },
];

export function SettingsPanel() {
  const { preference, set } = useTheme();
  const [devices, setDevices] = useState<Device[] | null>(null);

  useEffect(() => {
    devicesApi
      .list()
      .then(setDevices)
      .catch(() => setDevices([]));
  }, []);

  return (
    <div className="scroll-thin h-full overflow-y-auto">
      <div className="glass sticky top-0 z-10 border-b border-line px-6 py-5">
        <h1 className="text-title font-bold">Settings</h1>
      </div>

      <div className="mx-auto max-w-2xl space-y-8 p-6">
        <section>
          <h2 className="mb-3 text-caption font-semibold uppercase tracking-wider text-ink-3">
            Appearance
          </h2>
          <div className="grid grid-cols-3 gap-2">
            {THEMES.map((option) => (
              <button
                key={option.value}
                type="button"
                onClick={() => set(option.value)}
                aria-pressed={preference === option.value}
                className={
                  preference === option.value
                    ? 'flex flex-col items-center gap-2 rounded-xl border-2 border-brand bg-brand-soft py-4 text-callout'
                    : 'flex flex-col items-center gap-2 rounded-xl border border-line bg-surface py-4 text-callout text-ink-2 hover:bg-raised'
                }
              >
                <option.icon size={20} />
                {option.label}
              </button>
            ))}
          </div>
        </section>

        <section>
          <h2 className="mb-3 text-caption font-semibold uppercase tracking-wider text-ink-3">
            Notifications
          </h2>
          <div className="rounded-xl border border-line bg-surface p-4">
            <div className="flex items-start gap-3">
              <Bell size={18} className="mt-0.5 shrink-0 text-ink-3" />
              <p className="text-footnote text-ink-2">
                Tones, quiet hours and previews are set on your phone and apply everywhere,
                including here. Open{' '}
                <span className="font-semibold text-ink">You → Notifications</span> in the
                mobile app.
              </p>
            </div>
          </div>
        </section>

        <section>
          <h2 className="mb-3 text-caption font-semibold uppercase tracking-wider text-ink-3">
            Linked devices
          </h2>
          <div className="divide-y divide-line overflow-hidden rounded-xl border border-line bg-surface">
            {devices === null ? (
              <div className="flex items-center justify-center py-8">
                <Spinner />
              </div>
            ) : devices.length === 0 ? (
              <p className="px-4 py-4 text-callout text-ink-3">No other devices.</p>
            ) : (
              devices.map((device) => {
                const Icon = PLATFORM_ICON[device.platform] ?? Smartphone;
                return (
                  <div key={device.id} className="flex items-center gap-3 px-4 py-3">
                    <Icon size={18} className="shrink-0 text-ink-3" />
                    <div className="min-w-0 flex-1">
                      <p className="truncate text-callout">
                        {device.device_name ?? device.platform}
                      </p>
                      <p className="text-caption text-ink-3">
                        {device.is_primary ? 'Primary device' : device.platform}
                      </p>
                    </div>
                    {device.is_primary && (
                      <span className="rounded-full bg-brand-soft px-2 py-0.5 text-caption text-brand">
                        Phone
                      </span>
                    )}
                  </div>
                );
              })
            )}
          </div>
          <p className="mt-2 text-footnote text-ink-3">
            Devices are managed from your phone. Sign this browser out from{' '}
            <span className="font-medium text-ink-2">You → Linked devices</span> there, or from
            the account menu here.
          </p>
        </section>

        <section className="flex items-start gap-3 rounded-xl bg-brand-soft p-4">
          <ShieldCheck size={18} className="mt-0.5 shrink-0 text-brand" />
          <p className="text-footnote text-ink-2">
            This browser holds a session, never your encryption keys. Messages are decrypted
            locally and your phone can revoke this device at any time.
          </p>
        </section>
      </div>
    </div>
  );
}
