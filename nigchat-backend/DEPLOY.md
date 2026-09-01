# Deploying NigChat

Target layout:

```
api.nigchat.com     Render        the backend
app.nigchat.com     Vercel        the web client
nigchat.com         Vercel        landing page, or redirect to app.
```

Order matters — the database and Redis must exist before the backend starts, and
the backend must exist before the web client can be configured.

---

## 1. Database — Supabase

**You do not run migrations manually.** The server applies everything in
`migrations/` on boot, inside an advisory lock, so any number of instances can
start at once and exactly one applies them. Set `DATABASE_URL` and deploy; watch
for `migrations applied` in the logs.

1. Create the project. Choose the region with the lowest latency from Lagos —
   **measure it before committing**. West African traffic often routes north via
   subsea cable, so Frankfurt or London frequently beats Cape Town. Once a
   project exists, its region cannot be changed.

2. **Settings → Database → Connection string.** You will see several. This
   choice matters more than it looks:

   | Option | Use it? |
   |---|---|
   | **Direct** `db.<ref>.supabase.co:5432` | First choice. Full Postgres, prepared statements work. |
   | **Session pooler** `aws-0-<region>.pooler.supabase.com:5432` | Fall back to this. Also prepared-statement safe. |
   | **Transaction pooler** `...:6543` | **Do not use.** It breaks prepared statements, which sqlx relies on for every query. |

   Try direct first. If Render cannot reach it, the usual cause is that
   Supabase serves direct connections over IPv6 while the platform only has
   IPv4 egress — switch to the **session** pooler, not the transaction one.

3. Append `?sslmode=require`. Supabase requires TLS and the connection fails
   without it.

4. In the Supabase dashboard, turn off **Auth**, **Storage** and **Realtime**.
   You are not using any of them, and leaving them enabled is unnecessary
   attack surface on a project holding your users' data.

---

## 2. Redis — Upstash

Presence, rate limiting and cross-instance realtime fan-out all depend on it.
The backend will not start without it.

1. Create a Redis database. Same region as the backend, not the same as
   Supabase — this one sits on the hot path of every message.
2. Copy the connection string. It begins with **`rediss://`** — two `s`, meaning
   TLS. The backend is built with the TLS feature, so this works as-is.
3. Free tier is enough to launch. Watch the daily command limit: presence writes
   on every socket heartbeat, so it scales with connected users rather than
   messages.

---

## 3. Backend — Render

`render.yaml` is committed. In Render: **New → Blueprint**, point it at the repo,
and it reads the file.

### Plan

**Starter, not Free.** The free tier sleeps after inactivity, which for a chat
app means every WebSocket drops and the first request after a sleep takes about
thirty seconds. That is not a cost saving, it is a broken product.

### Build

Rust builds are slow — expect **10–20 minutes** for a first deploy. The
Dockerfile caches dependencies as a separate layer, so subsequent deploys that
only change your code are much faster. If the build times out, the fix is a
larger instance for the build, not a smaller binary.

### Environment

Set these in the dashboard. `render.yaml` marks the secrets `sync: false`, so
Render prompts rather than storing them in the repo.

```
DATABASE_URL           postgresql://postgres:PW@db.REF.supabase.co:5432/postgres?sslmode=require
DATABASE_MAX_CONNECTIONS   10
REDIS_URL              rediss://default:PW@xxx.upstash.io:6379

JWT_SECRET             (Render generates)
HASH_PEPPER            (Render generates — must differ from JWT_SECRET)

ENVIRONMENT            production
OTP_DEBUG_ECHO         false
ENABLE_DOCS            false
TRUST_PROXY_HEADERS    true

CORS_ALLOWED_ORIGINS   https://app.nigchat.com,https://nigchat.com

SMS_ENDPOINT           https://v4.api.termii.com/api/sms/send
SMS_API_KEY            your Termii key
SMS_SENDER_ID          NigChat
```

Four of these are enforced at boot and the server **refuses to start** if they
are wrong, which is deliberate — a misconfigured production deploy should fail
loudly rather than run insecurely:

- `OTP_DEBUG_ECHO=true` outside development is a full account-takeover hole.
- Missing `SMS_ENDPOINT` or `SMS_API_KEY` in production means nobody can sign in.
- `JWT_SECRET` equal to `HASH_PEPPER`, or either under 32 characters.
- The sample `change_me…` values.

`TRUST_PROXY_HEADERS=true` is correct **here specifically**: Render terminates
TLS at its edge and overwrites `X-Forwarded-For`. On a directly reachable server
this same setting would let anyone forge their IP and walk past the per-IP OTP
limit.

`DATABASE_MAX_CONNECTIONS=10` — hosted Postgres caps connections far below a
local box, and the pool will happily exhaust them.

### Custom domain

Render → your service → **Settings → Custom Domains → Add `api.nigchat.com`**.
It gives you a target host. At your DNS provider:

```
CNAME   api   <your-service>.onrender.com
```

Render issues the certificate automatically once DNS resolves — usually minutes,
occasionally an hour. **Wait for it to go green before configuring the web
client**, because a `wss://` connection to a domain without a valid certificate
fails silently in browsers with no useful error.

### Verify

```
curl https://api.nigchat.com/healthz
curl https://api.nigchat.com/readyz
```

`/readyz` reports Postgres and Redis separately, so a failure tells you which.

---

## 4. Web — Vercel

Environment variables in the Vercel project:

```
VITE_API_URL   https://api.nigchat.com
VITE_WS_URL    wss://api.nigchat.com
```

**`wss://`, not `ws://`.** A secure page cannot open an insecure WebSocket;
browsers block it as mixed content and the socket silently never connects.

Then **Settings → Domains → add `app.nigchat.com`**:

```
CNAME   app   cname.vercel-dns.com
```

For the apex, either point `nigchat.com` at Vercel too with a landing page, or
redirect it to `app.` — Vercel does apex domains with an A record it gives you.

`nigchat-web.vercel.app` keeps working. Leave it as a staging URL, but **do not
put it in `CORS_ALLOWED_ORIGINS`** for production — every origin on that list is
a site allowed to make credentialed calls for your signed-in users.

Vercel builds this in under a minute; it is a static bundle.

---

## 5. Mobile app

Point it at production and rebuild:

```
EXPO_PUBLIC_API_URL=https://api.nigchat.com
EXPO_PUBLIC_WS_URL=wss://api.nigchat.com
```

Then build with EAS:

```bash
npm install -g eas-cli
eas login
eas build:configure
eas build --platform android --profile production
eas build --platform ios --profile production
```

**Start the store submissions now, in parallel with everything else.** Play
review is typically a few days. App Store is one to three weeks, and first
submissions from a new account are frequently rejected on details that have
nothing to do with your code. That calendar time is not recoverable by working
harder.

---

## Two things that were wrong before this

Both would have failed on Render, and both are fixed in this version:

**The Dockerfile pinned Rust 1.79** — the exact version that failed on your
machine with `feature edition2024 is required`. Local builds used
`rust-toolchain.toml` and were fine; the Docker build had its own hardcoded
version. Now `rust:1-slim`.

**The Redis client had no TLS support.** Upstash and every other hosted Redis
serve `rediss://`, and without the `tokio-rustls-comp` feature the client
rejects that scheme outright. It worked locally only because the Docker Redis is
plaintext.

---

## Order of operations

1. Supabase project → connection string
2. Upstash Redis → connection string
3. Termii account → API key (sender ID approval takes a day or two; use their
   generic sender meanwhile)
4. Render blueprint → set env → deploy → wait for `migrations applied`
5. `api.nigchat.com` DNS → wait for the certificate
6. Vercel env + `app.nigchat.com`
7. `CORS_ALLOWED_ORIGINS` on Render → redeploy
8. Rebuild the app against production
9. Sign in on your own phone before telling anyone else

---

## Before real users

- Hide the 18 inert controls listed in `FEATURE-STATUS.md`. A button that does
  nothing reads as a broken app.
- **Fix the onboarding copy.** It currently says *"Every message is encrypted on
  your device."* That is not true yet — the transport carries base64, which is
  encoding, not encryption. Either build E2EE or change the sentence. Shipping
  that claim is the kind of thing that attracts exactly the wrong attention.
- Take a database backup before announcing. Supabase does daily backups on paid
  plans; on free, `pg_dump` it yourself.
EOF
