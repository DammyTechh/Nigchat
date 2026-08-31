# NigChat — Web

Browser client. Paired from the phone, never signed into directly.

Same design tokens, type scale and bubble geometry as the mobile app — see
`../nigchat-app/DESIGN.md`.

---

## Run it

```bash
npm install
cp .env.example .env      # point VITE_API_URL at your backend
npm run dev               # http://localhost:5173
```

The dev server proxies `/v1` (including the WebSocket) to the API, so there is no CORS
setup for local work. For a deployed build, set `VITE_API_URL` and `VITE_WS_URL` and add
the web origin to the backend's `CORS_ALLOWED_ORIGINS`.

```bash
npm run build     # static output in dist/
npm run typecheck
```

---

## Why Vite, not Next.js

Every route here is authenticated and every message is end-to-end encrypted, so there is
nothing for a server to render — no SEO surface, no shareable public page. Next would add
a server to operate and a hydration boundary to reason about, for no benefit. A static
SPA behind a CDN is the right shape.

---

## Pairing

The phone is the root of trust. This browser is granted a **device session** the phone
authorises and can revoke; it never receives the account's long-term keys. That is why
there is no password form — a password on a web page is phishable, and a stolen one would
hand over the whole account.

```
web asks for a pairing code
   → renders it as a QR
      → phone scans and confirms
         → web receives a token pair and continues
```

The code expires after 60 seconds, deliberately: a pairing QR left on an unattended
screen is an account takeover waiting to happen, and regenerating costs one click.

**Status:** the screen, polling loop, countdown and expiry are complete. The two network
calls are marked `TODO` in `src/routes/Pair.tsx` because the backend's device-linking
endpoints are not built yet — the `device_link_requests` table exists, the routes do not.
Wiring them is roughly ten lines.

---

## Layout

```
src/
├── main.tsx            entry
├── App.tsx             paired ? Workspace : PairScreen
├── routes/
│   ├── Pair.tsx        QR pairing
│   └── Workspace.tsx   two-pane shell
├── components/
│   ├── Rail.tsx             vertical nav + account menu + theme toggle
│   ├── ConversationList.tsx list, search, filters
│   ├── ChatPane.tsx         bubbles, composer, day separators
│   ├── SettingsPanel.tsx    appearance, linked devices
│   └── primitives.tsx       Avatar, Button, Badge, EmptyState…
├── lib/                api client, socket, base64, formatting, theme
├── store/              zustand: session, chats
└── styles/index.css    design tokens as CSS variables
```

---

## Details worth knowing

**Theme is applied before first paint.** A small script in `index.html` reads the stored
preference and sets the class on `<html>` before React mounts, so a dark-mode user never
sees a white flash. Doing it in React would be one render too late. Setting it to
`System` follows the OS live, without a reload.

**Base64 goes through `TextEncoder`.** The browser's `btoa` is Latin-1 only and throws on
an emoji or a diacritic. This also keeps the web byte-compatible with the mobile client —
both must produce the same ciphertext for the same message.

**The socket handles a backgrounded tab.** Browsers throttle timers in hidden tabs, so a
socket can sit apparently open while nothing arrives. Reconnection is driven by
`visibilitychange` and `online` as well as by the heartbeat.

**Enter sends, Shift+Enter breaks the line.** The composer grows with its content to a
maximum height, then scrolls.

**Responsive is not a shrunken desktop.** Below 768px, selecting a conversation replaces
the list entirely with a back button, which is how a phone browser should behave.

**Accessibility.** Focus rings appear for keyboard users only, every icon button has a
label, the connection strip is a live region, and `prefers-reduced-motion` disables
animation.

---

## Not yet wired

- device-linking endpoints (above)
- media upload and display
- calls, status and channels — mobile only for now
- search inside a conversation
- encryption: `lib/base64.ts` marks where the Signal session goes; the wire format already
  carries ciphertext, so adding it does not change the API contract
