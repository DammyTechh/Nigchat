# NigChat — Mobile App

React Native (Expo) client for iOS, iPadOS, Android and Android tablets.

Design rationale is in **[DESIGN.md](./DESIGN.md)** — including how this avoids looking
like a clone while keeping the same feature set.

---

## Run it

```bash
npm install
cp .env.example .env      # point EXPO_PUBLIC_API_URL at your backend
npx expo start
```

Then press `i` for the iOS simulator, `a` for Android, or scan the QR with Expo Go.

**On a physical device, `localhost` is the phone itself.** Put your machine's LAN address
in `.env`:

```
EXPO_PUBLIC_API_URL=http://192.168.1.10:8080
EXPO_PUBLIC_WS_URL=ws://192.168.1.10:8080
```

The camera (QR pairing) needs a development build rather than Expo Go:

```bash
npx expo run:ios      # or run:android
```

---

## Layout

```
app/                        expo-router file-based routes
├── _layout.tsx             theme + auth gate + splash handoff
├── (auth)/                 welcome → phone → verify → profile
├── (tabs)/                 Chats · Updates · Calls · You
├── chat/[id].tsx           conversation
├── settings/               appearance, notifications, privacy, security, devices
├── link-device.tsx         QR scanner for web pairing
└── new-chat.tsx

src/
├── theme/                  tokens, typography, ThemeProvider
├── components/             Text, Icon, Avatar, Button, ListRow, MessageBubble…
├── api/                    HTTP client, endpoints, WebSocket
├── store/                  zustand: auth, chats
└── utils/                  formatting, phone handling

assets/images/              icons + splashes generated from the logo
```

---

## How it talks to the backend

**Auth.** Phone → OTP → access token (15 min) + refresh token (90 days). The client
refreshes automatically on a 401 and retries once, and refreshes are **single-flight** —
when six requests hit a stale token at once, only one refresh goes out. Without that,
five of them rotate an already-rotated token, which the backend correctly treats as theft
and revokes the whole device. Tokens live in SecureStore (Keychain / EncryptedSharedPreferences),
never AsyncStorage.

**Sending.** Every message carries a `client_message_id` generated on the device *before*
the request. Retrying with the same value returns the original instead of duplicating —
this is what makes a send safe to retry on a dropped connection. Messages render
optimistically and are reconciled by `seq` when the server replies; a failed send is
marked, never silently discarded.

**Ordering.** Everything sorts by `seq`, never `created_at`. A device clock a few seconds
off would otherwise make messages jump around.

**Realtime.** The socket is a fast path, not the source of truth. It reconnects with
exponential backoff plus jitter, and treats silence longer than 70s as a dead connection
— a mobile radio can drop a socket without either side seeing a close frame. Every
reconnect re-syncs from the last known `seq`, so a dropped connection can delay a message
but never lose one.

---

## Web pairing

The web client shows a QR code; this app scans it from **You → Linked devices → Scan
code**. The phone is the root of trust — the browser never receives the account's
long-term keys, which is why pairing works this way round rather than by typing a
password into a website.

The scanner screen is complete. The `POST /v1/devices/link` call is marked `TODO` because
the backend's device-linking endpoints are not built yet (`device_link_requests` exists in
the schema).

---

## Notes on dependencies

Every package in `package.json` is imported somewhere — verified by a scan, not
by eye. Versions are pinned to the Expo SDK 51 matrix, so `npx expo install
--check` should report nothing.

`expo-notifications` no longer supports remote push inside **Expo Go** (SDK 51+).
Everything else runs in Expo Go; push and the QR camera need a development
build:

```bash
npx expo run:ios     # or run:android
```

Android notification sounds are bound to **channels**, not to individual
messages, and a channel's sound cannot be changed after it is created. So each
selectable tone gets its own channel, created up front in `src/utils/push.ts` —
switching tones switches channels. Calls and security alerts get MAX-importance
channels with `bypassDnd`, matching the server's rule that those two categories
are never silenceable.

## Not yet wired

The UI is complete for these; the data is not.

- **Encryption.** `src/store/chats.ts` base64-encodes where the Signal session will
  encrypt. The wire format already carries ciphertext, so adding the crypto layer does not
  change the API contract.
- **Media.** Attach and voice buttons are present; upload needs the backend's presigned
  URL endpoints.
- **Calls.** Screens and history exist; signalling needs an SFU.
- **Status and channels.** Updates renders from local sample data.
- **Contacts.** `new-chat` is wired to `POST /v1/users/sync-contacts`; the device-side
  contact read and hashing still needs to be added.
- **Push registration.** `expo-notifications` is configured and the endpoint exists;
  the token handoff on launch is not yet called.

---

## Conventions

1. **All colour comes from `useColors()`.** No hex literals in screens — that is what
   keeps dark mode correct without auditing every file.
2. **All spacing comes from the 4pt scale** in `tokens.ts`.
3. **All text goes through `<Text variant>`.** No raw `<Text>` from React Native.
4. **All icons go through `<Icon name>`.** One set, one stroke weight.
5. **Screens deserialise, call a store, and render.** Business rules live in the store or
   on the server.
