import { Laptop, Lock, RefreshCw, ShieldCheck, Smartphone } from 'lucide-react';
import QRCode from 'qrcode';
import { useCallback, useEffect, useRef, useState } from 'react';

import { Button, Spinner } from '../components/primitives';
import { deviceLinks } from '../lib/endpoints';
import { useTheme } from '../lib/theme';
import { useSession } from '../store/session';

/**
 * Pairing.
 *
 * The phone is the root of trust. The browser never receives the account's
 * long-term keys — it is granted a *device session* that the phone authorises
 * and can revoke at any time. That is why pairing works this way round rather
 * than by typing a password into a website: a password on a web form is
 * phishable, and a stolen one would give an attacker the account outright.
 *
 * Flow:
 *   1. this page asks the backend for a short-lived pairing code
 *   2. it renders the code as a QR
 *   3. the phone scans it and confirms
 *   4. this page, which has been polling, receives a token pair and continues
 *
 * The code expires after 60 seconds. That is deliberately short — a pairing QR
 * left on an unattended screen is an account takeover waiting to happen, and a
 * regenerate button costs one click.
 */

const CODE_LIFETIME_SECONDS = 60;

/** A human-readable name for this browser, shown on the phone before approval. */
function describeBrowser(): string {
  const ua = navigator.userAgent;
  const browser = /Edg\//.test(ua)
    ? 'Edge'
    : /Chrome\//.test(ua)
      ? 'Chrome'
      : /Safari\//.test(ua)
        ? 'Safari'
        : /Firefox\//.test(ua)
          ? 'Firefox'
          : 'Browser';

  const os = /Windows/.test(ua)
    ? 'Windows'
    : /Mac OS/.test(ua)
      ? 'macOS'
      : /Linux/.test(ua)
        ? 'Linux'
        : /Android/.test(ua)
          ? 'Android'
          : 'this device';

  return `${browser} on ${os}`;
}

type PairState = 'requesting' | 'waiting' | 'expired' | 'confirming' | 'error';

export default function PairScreen() {
  const adopt = useSession((state) => state.adopt);
  const { isDark } = useTheme();

  const [state, setState] = useState<PairState>('requesting');
  const [qrDataUrl, setQrDataUrl] = useState<string | null>(null);
  const [secondsLeft, setSecondsLeft] = useState(CODE_LIFETIME_SECONDS);
  const [error, setError] = useState<string | null>(null);
  const pollTimer = useRef<number | null>(null);

  const stopPolling = useCallback(() => {
    if (pollTimer.current) window.clearInterval(pollTimer.current);
    pollTimer.current = null;
  }, []);

  const requestCode = useCallback(async () => {
    stopPolling();
    setState('requesting');
    setError(null);
    setSecondsLeft(CODE_LIFETIME_SECONDS);

    try {
      // Name the browser so the phone can show what it is about to authorise.
      const { code, expires_in } = await deviceLinks.request(describeBrowser());
      setSecondsLeft(expires_in);

      // The QR carries the raw code and nothing else. Anything extra would
      // only be another thing to validate on the phone.
      const dataUrl = await QRCode.toDataURL(code, {
        errorCorrectionLevel: 'M',
        // Chunky and high-contrast: this is read by a phone camera at arm's
        // length, often at an angle, sometimes on a dim laptop screen.
        margin: 2,
        width: 480,
        color: {
          dark: isDark ? '#E9F1EC' : '#131A16',
          light: isDark ? '#0B120E' : '#FFFFFF',
        },
      });

      setQrDataUrl(dataUrl);
      setState('waiting');

      pollTimer.current = window.setInterval(async () => {
        try {
          const result = await deviceLinks.poll(code);

          if (result.status === 'approved' && result.access_token) {
            stopPolling();
            setState('confirming');
            await adopt({
              access_token: result.access_token,
              refresh_token: result.refresh_token!,
              user_id: result.user_id!,
              device_id: result.device_id!,
            });
            return;
          }

          if (result.status === 'gone') {
            stopPolling();
            setState('expired');
          }
        } catch {
          // A dropped poll is not fatal — the interval simply tries again.
          // Only the countdown ends the attempt.
        }
      }, 2_000);
    } catch {
      setState('error');
      setError('Could not reach NigChat. Check your connection and try again.');
    }
  }, [adopt, isDark, stopPolling]);

  useEffect(() => {
    requestCode();
    return stopPolling;
  }, [requestCode, stopPolling]);

  // Countdown. Expiry is enforced server-side too — this is the visible half.
  useEffect(() => {
    if (state !== 'waiting') return;

    const timer = window.setInterval(() => {
      setSecondsLeft((value) => {
        if (value <= 1) {
          stopPolling();
          setState('expired');
          return 0;
        }
        return value - 1;
      });
    }, 1_000);

    return () => window.clearInterval(timer);
  }, [state, stopPolling]);

  return (
    <div className="flex min-h-full flex-col">
      <header className="flex items-center gap-3 px-6 py-5 sm:px-10">
        <img src="/logo-mark.png" alt="" className="h-8 w-8" />
        <span className="text-headline">NigChat</span>
      </header>

      <main className="flex flex-1 items-center justify-center px-6 pb-16">
        <div className="grid w-full max-w-4xl gap-12 lg:grid-cols-[1.1fr_1fr] lg:items-center">
          {/* Instructions first in the DOM: on a narrow window it reads as a
              sensible sequence, and screen readers get the explanation before
              the image. */}
          <div>
            <h1 className="text-display">Use NigChat on this computer</h1>
            <p className="mt-3 max-w-md text-body text-ink-2">
              Your phone stays in charge. It keeps the keys and can sign this browser out
              at any time.
            </p>

            <ol className="mt-8 space-y-5">
              {[
                { icon: Smartphone, text: 'Open NigChat on your phone.' },
                { icon: Laptop, text: 'Go to You → Linked devices → Scan code.' },
                { icon: ShieldCheck, text: 'Point your camera at the code on this screen.' },
              ].map((step, index) => (
                <li key={index} className="flex gap-4">
                  <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-xl bg-brand-soft">
                    <step.icon size={18} className="text-brand" strokeWidth={1.9} />
                  </span>
                  <span className="pt-1.5 text-callout text-ink-2">
                    <span className="mr-2 font-semibold text-ink">{index + 1}.</span>
                    {step.text}
                  </span>
                </li>
              ))}
            </ol>

            <p className="mt-8 flex items-start gap-2 text-footnote text-ink-3">
              <Lock size={14} className="mt-0.5 shrink-0" />
              Messages stay end-to-end encrypted. Pairing grants this browser a session,
              never your keys.
            </p>
          </div>

          {/* The code panel. Glass over the page so it reads as a surface
              floating above the explanation rather than a second column. */}
          <div className="glass rounded-3xl border border-line p-6 shadow-subtle sm:p-8">
            <div className="relative mx-auto aspect-square w-full max-w-[320px]">
              {qrDataUrl && state === 'waiting' ? (
                <img
                  src={qrDataUrl}
                  alt="Pairing code"
                  className="h-full w-full rounded-2xl animate-fade-up"
                />
              ) : (
                <div className="flex h-full w-full items-center justify-center rounded-2xl bg-raised">
                  {state === 'requesting' || state === 'confirming' ? (
                    <Spinner className="h-7 w-7" />
                  ) : null}
                </div>
              )}

              {/* Expiry is an overlay rather than a replacement, so the layout
                  never jumps and the regenerate button lands where the eye
                  already is. */}
              {state === 'expired' && (
                <div className="absolute inset-0 flex flex-col items-center justify-center gap-4 rounded-2xl bg-bg/85 backdrop-blur-sm">
                  <p className="text-callout text-ink-2">This code has expired</p>
                  <Button icon={RefreshCw} onClick={requestCode}>
                    New code
                  </Button>
                </div>
              )}
            </div>

            <div className="mt-6 text-center">
              {state === 'waiting' && (
                <p className="text-footnote text-ink-3">
                  Expires in{' '}
                  <span className="tabular-nums font-semibold text-ink-2">{secondsLeft}s</span>
                </p>
              )}
              {state === 'confirming' && (
                <p className="text-footnote text-brand">Confirmed — signing you in…</p>
              )}
              {state === 'error' && <p className="text-footnote text-danger">{error}</p>}
            </div>
          </div>
        </div>
      </main>

      <footer className="px-6 pb-8 text-center text-caption text-ink-3 sm:px-10">
        Keep this code private. Anyone who scans it can read your messages until you sign
        the device out.
      </footer>
    </div>
  );
}
