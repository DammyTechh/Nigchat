# Running the app

Expo SDK 54.

## Install — in this order

The previous `npm install` left a partially-written `node_modules`, which is why
Babel could not find its own preset. Start from a clean tree:

```bash
# Windows PowerShell
Remove-Item -Recurse -Force node_modules, package-lock.json, .expo -ErrorAction SilentlyContinue

npm install
npx expo install babel-preset-expo
npx expo start -c
```

```bash
# macOS / Linux
rm -rf node_modules package-lock.json .expo
npm install
npx expo install babel-preset-expo
npx expo start -c
```

`npx expo install` is used for that one package rather than a pinned version in
`package.json`, because Expo reads the SDK's own compatibility table and writes
the exact version SDK 54 wants. Guessing it here would be one more thing to get
wrong.

Do **not** run `npx expo install --fix` before the first successful install —
it runs `npm install` internally, and on a broken tree it fails and leaves
things worse.

---

## What was wrong, and what changed

**1. `Cannot find module 'babel-preset-expo'`**
It arrives as a dependency of `expo`, so it is normally present without being
listed. The earlier `npm install` aborted partway through and left it out. A
clean reinstall restores it; installing it explicitly makes it immune to this
happening again.

**2. `ERESOLVE` on react-dom**
`expo-router` declares `react-dom` as an *optional* peer with the range `"*"`,
so npm installed the newest release — 19.2.8 — whose own peer demands
`react@^19.2.8`. SDK 54 pins React at 19.1.0, so the tree could not resolve.
`react-dom` is now pinned to 19.1.0 to match React exactly.

**3. `Asset not found: assets/icon.png`**
This one was mine. Assets lived in `assets/images/`, but something in the
toolchain was resolving the conventional `assets/` root. Rather than keep
guessing which layout each tool expects, every asset now lives in a **single
directory**, `assets/`, and both `app.json` and the two `require()` calls point
there. The path that was being requested now exists.

**4. Deprecation and audit noise**
Most of it came from ESLint 8, which drags in the abandoned
`glob`/`rimraf`/`inflight` chain. Upgraded to ESLint 9 with
`eslint-config-expo@10`, and `.eslintrc.js` was replaced by `eslint.config.js` —
ESLint 9 no longer reads the old format.

**5. Version alignment**
`react-native` 0.81.4 → 0.81.5 and `eslint-config-expo` → `~10.0.0`, the two
things expo-doctor asked for.

---

## Verify before bundling

```bash
npx expo config --type public | grep -i icon
```

Expect `./assets/icon.png`. If you get anything else, a stray `app.config.js`
or `app.config.ts` is overriding `app.json` — those take precedence silently.

---

## First run

`.env` is already set to **192.168.1.62**, this machine on the office Wi-Fi.
Re-run `ipconfig` and update both lines if that changes.

### No SMS provider is needed

With `OTP_DEBUG_ECHO=true` on the backend, the verification code is returned in
the API response instead of being sent by SMS. The app reads it and **fills the
six boxes in automatically**, then submits — so signing in is: type a number,
tap Continue, and you are through. Nothing to copy.

Any phone number works. It does not have to be real or reachable; the backend
only checks that it is valid E.164 (`+` and 7–15 digits).

Before shipping anywhere public, set `OTP_DEBUG_ECHO=false` and configure an SMS
provider. The backend **refuses to start** with the flag on unless
`ENVIRONMENT=development`, so this cannot reach production by accident.

### One thing that will trip you up while testing

OTP requests are rate limited to **one per minute and five per hour per phone
number** — the limit that stops an attacker running up an SMS bill. If you sign
in repeatedly with the same number you will get a `429`. Either use a different
number each time (any digits work), or clear the counters:

```powershell
docker compose restart redis
```

### Firewall

If Metro serves the bundle but the app cannot reach the API, this is almost
always it. Run once, as Administrator:

```powershell
New-NetFirewallRule -DisplayName "NigChat API 8080" -Direction Inbound `
  -Protocol TCP -LocalPort 8080 -Action Allow -Profile Private
```

Check from the phone's browser before blaming the app:
**http://192.168.1.62:8080/healthz** should return
`{"status":"ok","service":"nigchat-api"}`.

The phone must be on the same Wi-Fi — a `192.168.1.x` address. A phone on
mobile data cannot see this machine.

## What needs a development build

Expo Go covers sign-in, chats, messaging, realtime, settings and dark mode.
These three need `npx expo run:android` or `run:ios`:

- push notifications — remote push was removed from Expo Go in SDK 53
- the QR scanner for web pairing
- Face ID / fingerprint unlock
