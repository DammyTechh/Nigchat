# NigChat Backend

One unified backend for every NigChat client — iOS, iPadOS, Android, Android tablet,
Web, Windows, macOS and Linux. Clients never connect to the database. Everything goes
through versioned HTTP endpoints and one authenticated WebSocket.

Rust · Axum · PostgreSQL · Redis · Clean Architecture · End-to-end encrypted

---

> **Windows: start with [RUNBOOK.md](./RUNBOOK.md)** — step by step, no `make`
> required, including secret generation, migrations and a smoke test.

## Run it in three commands

```bash
make setup     # copies .env, generates real secrets, starts Postgres + Redis
make run       # builds and starts the server; migrations apply automatically
open http://localhost:8080/docs
```

That is the whole local setup. `make setup` generates real random secrets rather than
leaving the placeholders in place, because the server refuses to boot with the sample
values outside development.

### Full stack in Docker

```bash
make up        # 2 API instances behind nginx, Postgres, Redis, MinIO
make logs
make down
```

Two instances by default, deliberately. The thing most likely to break in this
architecture is cross-instance realtime delivery — a message sent to instance A
reaching a socket held by instance B. Running two locally exercises that path every
day instead of discovering it in production.

### Verify

```bash
make check     # cargo check + clippy -D warnings
make test      # unit tests, no database required
```

**This has not been compiled in the environment where it was written** — no Rust
toolchain was available. Run `make check` first. If a crate's API has drifted, the
compiler error will point straight at it; the likeliest spots are the Redis pub/sub
call in `crates/api/src/ws.rs` and the `tower-http` layer names in
`crates/api/src/router.rs`.

---

## Requirements

| | |
|---|---|
| Rust | 1.85+ (the dependency tree uses edition 2024) |
| PostgreSQL | 16, with `citext` and `pg_trgm` |
| Redis | 7 |
| Docker | optional, for the full stack |

Everything else is a Cargo dependency.

---

## Layout

```
nigchat/
├── Cargo.toml                 workspace + shared dependency versions
├── Makefile                   every common task
├── Dockerfile                 multi-stage, non-root, health-checked
├── docker-compose.yml         2 API instances + Postgres + Redis + MinIO
├── .env.example               every setting, documented
├── migrations/
│   └── 0001_nigchat_init.sql  the entire schema, one file
├── deploy/nginx.conf          load balancer + per-IP rate limits
└── crates/
    ├── domain/                entities, rules, ports. No DB, no HTTP.
    ├── application/           use cases
    ├── infrastructure/        Postgres, Redis, FCM, APNs, SMS
    ├── api/                   handlers, DTOs, OpenAPI, WebSocket
    └── server/                composition root
```

### Why it is split this way

```
server ──> api ──> application ──> domain
   └────> infrastructure ───────────┘
```

Dependencies point inward, and the Cargo manifests enforce it — `crates/domain` has no
sqlx, no axum, no redis, and none may be added. Three payoffs:

1. **Business rules are testable with no containers.** The notification policy has 14
   tests that run in microseconds against no database.
2. **Swapping infrastructure touches one crate.** Redis Pub/Sub to Redpanda, or FCM to
   another provider, changes an adapter and zero business logic.
3. **Rules cannot hide in handlers.** A handler that only deserialises, calls a use
   case and serialises has nowhere to put a quiet `if`.

---

## API

Full interactive documentation at **`/docs`** (Swagger UI), generated from the same
annotations that sit on the handlers — so it cannot drift from the implementation.

| Area | Endpoints |
|---|---|
| Auth | `POST /v1/auth/request-otp` · `verify-otp` · `refresh` · `logout` |
| Profile | `GET/PATCH /v1/me` · `GET /v1/users/{id}` · `POST /v1/users/sync-contacts` |
| Blocking | `POST /v1/me/blocks` · `DELETE /v1/me/blocks/{id}` |
| Devices | `GET /v1/me/devices` · `DELETE /v1/me/devices/{id}` · `POST .../push-token` |
| Security | `GET /v1/me/security-events` |
| Conversations | `GET /v1/conversations` · `POST .../direct` · `.../group` · members · roles |
| Messages | `POST /v1/messages` · `GET /v1/conversations/{id}/messages` · edit · delete · reactions |
| Read state | `POST /v1/conversations/{id}/read` |
| Notifications | tones · preferences · per-conversation overrides · mute |
| Encryption | `POST /v1/keys` · `GET /v1/keys/{user_id}` · `GET /v1/keys/count` |
| Realtime | `GET /v1/ws?token=…` |
| System | `/healthz` · `/readyz` |

Errors are always `{"error": {"code", "message"}}`. **Branch on `code`, never on
`message`** — messages are for humans and may be reworded.

---

## Three things client developers must get right

**1. Always send `client_message_id`.**
Generate a UUID on the device *before* the request. Sending twice with the same value
returns the original message; it never creates a duplicate. This is what makes a send
safe to retry on a dropped connection, which on mobile networks is constant.

**2. Order and paginate by `seq`, never `created_at`.**
`seq` is a per-conversation counter that only increases. Timestamps come from whichever
server handled the write and will occasionally disagree. Store the highest `seq` you
hold per conversation — that value is your sync cursor and your read receipt.

**3. The socket is a fast path, not the source of truth.**
A dropped socket must never mean a lost message. On reconnect: `GET /v1/conversations`,
compare each `head_seq` against your local cursor, pull any gap with `?after_seq=`.
Build this on day one, not after the first bug report.

Socket frames are `{"type": "...", "data": {...}}`: `message_created`,
`message_edited`, `message_deleted`, `reaction_changed`, `read_receipt`,
`delivery_receipt`, `typing`, `presence`, `conversation_created`,
`membership_changed`, `call_signal`, `device_event`, `key_changed`, `sync_required`.

---

## Scaling

Every API instance is stateless. Add or kill instances at any moment; no client
notices.

**Sockets are the exception, and are handled explicitly.** A WebSocket lives on exactly
one instance, but the message that must reach it can be written by any instance.
Instances therefore never talk to each other — they publish to Redis Pub/Sub, and each
delivers to whichever sockets it owns locally. The `EventPublisher` trait is the seam:
replacing Redis with Redpanda for durable, replayable, multi-region fan-out changes one
adapter.

Known scaling levers, in the order they will be needed:

1. **Read replicas** for the conversation list, which is the heaviest read.
2. **Partition `messages`** by hash of `conversation_id`. The schema is already
   partition-ready — `(conversation_id, seq)` is in every hot query.
3. **Redpanda** in place of Redis Pub/Sub, once at-most-once delivery stops being
   acceptable or fan-out crosses regions.
4. **Split the realtime tier** into its own deployment. The module boundary already
   exists, so this is a deployment change rather than a rewrite.
5. **Outbox relay worker.** The `event_outbox` table is written transactionally with
   every message; the relay that drains it is not yet built.

Design choices that make these possible rather than painful: UUIDv7 keys (time-ordered,
so inserts append instead of fragmenting the index), keyset pagination everywhere (no
OFFSET, which at message 500,000 reads half a million rows to discard them), read state
as high-water marks (a 500-member group with 1M messages needs 500 rows, not 500
million), and batched presence and block lookups (a large fan-out is two queries, not
1,500).

---

## Security

| | |
|---|---|
| Transport | TLS enforced; HSTS, CSP, `nosniff`, `DENY` framing, `no-store` on every response |
| Sessions | 15-minute access JWT; 90-day refresh token, rotated on every use |
| Two-step PIN | Argon2id, 5 attempts/hour, trivial PINs rejected, failures on the security timeline |
| Token theft | Presenting a spent refresh token revokes **every session on that device** |
| Secrets at rest | Argon2id for PINs; HMAC-SHA256 under a server pepper for tokens, OTPs, phone and IP hashes |
| OTP | Attempt-capped, single-use, constant-time compare, never stored |
| Rate limits | Per phone (burst + hourly), **per IP**, per user, plus per-IP limits in nginx |
| Proxy headers | `X-Forwarded-For` believed only when `TRUST_PROXY_HEADERS=true` |
| Resource caps | 10 devices per account, 12 sockets per user per instance |
| Authorization | The `CurrentUser` extractor is the only source of caller identity — a handler cannot accidentally trust a body field |
| Enumeration | "Not a member" and "no such conversation" return the identical error |
| Contacts | Discovery takes hashed numbers, so the server never learns non-users' numbers |
| Audit | Append-only `security_events` (user-visible) and `admin_audit_logs` |
| Boot checks | The server refuses to start with short secrets, sample secrets, `OTP_DEBUG_ECHO` outside development, or no SMS provider in production |

The per-IP OTP limit deserves a note: without it, one host can pump SMS charges across
thousands of different numbers while never tripping any per-number limit. That is a
real invoice, not a theoretical risk.

---

## Encryption and its consequences

Message bodies are ciphertext produced on the device. `messages.ciphertext` is `BYTEA`
and there is no plaintext column for user messages at all — "never log message bodies"
is a property of the schema rather than a rule someone must remember. The server is a
key *directory*: it stores public identity keys, signed prekeys and one-time prekeys,
hands out bundles, and holds nothing that would let it read a message.

Three consequences the client teams must plan for:

1. **Server-side search is impossible.** Search runs on-device over the local decrypted
   database. Any server-side index can only ever cover metadata — names, titles,
   membership.
2. **Push notifications carry no message text.** The server sends a template; the
   device decrypts and renders the real content locally (iOS notification service
   extension, Android channel).
3. **Link previews must be generated by the sender**, client-side, and shipped inside
   the ciphertext.

This is how Signal and WhatsApp work. Each is invisible from the backend side and
expensive to discover late.

---

## Notifications

Push is a first-class domain concern, not glue inside an FCM client. The decision logic
lives in `crates/domain/src/notifications.rs` — pure, no database, no clock of its own,
14 unit tests.

Rules implemented and covered:

- a muted conversation is silent, **but an @mention cuts through** — unless the user
  turned that off for that specific chat
- quiet hours are evaluated in the recipient's **local** time, including the 22:00–07:00
  wrap-around that gets written wrong (start inclusive, end exclusive)
- calls may ring through quiet hours; messages never do
- security and device-link alerts cannot be disabled or quieted — an attacker who can
  mute the alerts can take an account silently
- an online recipient gets no push, **except** a ringing call, which must alert even
  with the app open
- own messages, blocked senders and missing tokens are suppressed with a **recorded
  reason**, so "why didn't I get notified?" is answerable from
  `notification_deliveries` with a fact rather than a guess

**Tones** resolve through a three-step chain: per-conversation tone → account tone for
that category → client default. Twelve are seeded across message, group, call, status
and security categories; the client bundles the audio and the server stores the
identifier, so adding a tone is a data change rather than an app release. Android
carries the tone as a notification channel (sound is bound to the channel on Android 8+,
not to the message); iOS carries it as a `.caf` file name.

Dispatch never blocks a send — it is spawned, so a slow APNs connection cannot add
latency to the sender's response. Delivery is idempotent: the ledger's unique constraint
means a retried dispatch cannot buzz a phone twice. Dead tokens are retired
automatically on APNs 410 / FCM 404, and after ten consecutive transient failures.

---

## Going to production

**[PRODUCTION-PLAN.md](./PRODUCTION-PLAN.md)** — what ships this week, what each
remaining feature actually costs, and concrete picks for SMS, database and
hosting. It starts with an honest timeline, including where the "one week" ask
does not survive contact with the work.

`render.yaml` is committed and ready; secrets are prompted for in the dashboard.

## Audit

`AUDIT.md` records a full review of this codebase: twelve defects found and fixed
(including two that would not compile, a SQL parameter gap that would have broken every
message deletion, and an `X-Forwarded-For` trust issue that defeated the per-IP OTP
limit), plus the structural checks that now pass and the limitations worth a decision.

## Not yet built

Tables exist for all of these; the endpoints do not.

- media upload sessions (presigned URLs) and the orphan sweeper
- communities, channels and status endpoints
- call signalling routes (an SFU such as LiveKit is a separate service)
- reports and moderation tooling
- the outbox relay worker, and the disappearing-message and status reapers
- passkey registration and QR device linking
- account deletion and data export (NDPR / GDPR)
- integration tests against a live database

---

## Conventions to preserve

1. **No state in the process.** If you are about to add a field to `ApiState`, stop — it
   will work on one instance and break on two.
2. **Media never transits the API.** Presigned URLs only.
3. **Never trust a user id from a request body.** `CurrentUser` is the only source.
4. **Anything spanning more than one table runs in a transaction.**
5. **`/v1` is frozen once a client ships.** Add `/v2` and run both.
6. **Product rules go in `domain` or `application`, never in a handler or an adapter.**
   If you are writing `if muted` in the push adapter, it belongs in the policy.
