# Deep Scan — Audit Report

Full review of the backend against the specification and against what would actually
happen in production. Twelve defects found, all fixed. What remains unbuilt is listed
honestly at the end.

---

## Defects found and fixed

### Would not compile

**1. `AuthenticatedSession { ..session }` across two different types**
`crates/application/src/auth.rs` — `issue_session()` returns `IssuedSession`, and
struct-update syntax requires both sides to be the same type. Every sign-in path went
through this line. Now converts explicitly through the existing `From` impl.

**2. `Uuid` used inside a `utoipa::path` macro without being imported**
`crates/api/src/routes/messages.rs` — the `params(("message_id" = Uuid, Path,))`
annotations reference a type that was never brought into scope. Four routes affected.

### Would fail at runtime

**3. SQL parameter gap in message deletion**
`postgres/conversations.rs::soft_delete` bound three values but the statement used
`$1` and `$3`, skipping `$2`. PostgreSQL requires `$1..$n` to be contiguous, so **every
delete would have failed** the moment it was called. Found by scanning every query for
bind-count-versus-placeholder mismatches; this was the only one.

**4. Invalid prekey handout query**
`postgres/keys.rs::take_prekey_bundles` used `DELETE ... USING LATERAL` with the lateral
subquery referencing the DELETE's own target table, which PostgreSQL rejects
("invalid reference to FROM-clause entry"). Rewritten with a `DISTINCT ON` CTE. The new
form is also safe under concurrency: `RETURNING` only yields rows that statement
actually deleted, so two senders can never be handed the same one-time key — the loser
simply gets a bundle without one.

**5. Contact discovery could only ever return an empty list**
`find_by_phone_hashes` matched against the `contacts` table, and **nothing in the
codebase ever inserts into `contacts`**. The feature was wired end to end and silently
returned nothing. Fixed by storing the peppered `phone_hash` on `users` at
registration and matching against that — which is also the correct privacy design,
since the server still never receives raw numbers of non-users.

### Security

**6. `X-Forwarded-For` was trusted unconditionally**
The per-IP OTP limit is what stops SMS-pumping fraud — an attacker billing you for
messages across thousands of numbers while never tripping any per-number limit. Because
the header was believed without question, a directly reachable server let an attacker
rotate their apparent IP freely and walk straight past it. Now gated behind
`TRUST_PROXY_HEADERS`, default **false**; enabled in `docker-compose.yml` where nginx
overwrites the header before it reaches the API.

**7. Client-supplied mentions were never validated**
A sender could name any user id at all. Non-members were written to
`message_mentions` — noise at best, a membership-probing oracle at worst. Now filtered
against the active member list, which the send path already loads.

**8. Group-invite privacy setting was ignored**
The comment claimed invitees' "who can add me to groups" was respected; the code only
checked blocks. Now enforced, and enforced on `add_members` too — otherwise the rule is
bypassed by creating an empty group and adding afterwards. Refusals are silent, because
naming which contacts declined would leak their privacy settings.

**9. No cap on linked devices** — an account could accumulate unbounded devices, meaning
unbounded push fan-out and unbounded blast radius on compromise. Capped at 10.

**10. No cap on WebSocket connections** — a client reconnecting in a loop without
closing could pin unbounded memory and file descriptors on one instance. Capped at 12
per user per instance, with a clean close frame so the client backs off.

### Features wired but unreachable

**11. Delivery receipts** — `advance_delivery_marker` and the `DeliveryReceipt` event
both existed, with no endpoint to reach them. The "delivered" tick was therefore
impossible to implement on any client. Added `POST /v1/conversations/{id}/delivered`.

**12. Reply notifications never fired** — the dispatcher hardcoded
`is_reply_to_recipient: false`, so replying to someone was indistinguishable from a
plain message and could not cut through a mute. Now resolved from the parent message's
author, which the send path already fetches for validation.

**13. Two-step verification PIN was schema-only** — `set_two_step_pin`,
`two_step_pin_hash`, `hash_secret` and `verify_secret` were declared and implemented but
never called from anywhere. This is the control that stops a SIM-swap attacker taking an
account with nothing but a hijacked SMS, so it mattered. Now implemented with Argon2id,
a 5-per-hour attempt limit, trivial-PIN rejection (repeats and runs), the current PIN
required to change or disable it, and every failure recorded on the user's own security
timeline.

**14. Username lookup** — `find_by_username` had no caller. Added
`GET /v1/users/by-username/{username}`, rate limited because handle lookup is an
enumeration surface.

---

## Verified clean

| Check | Result |
|---|---|
| SQL bind-parameter contiguity, every query | clean |
| Every `#[utoipa::path]` handler registered in the OpenAPI document | 43/43 |
| Every registered route resolves to a handler that exists | 44/44 |
| `domain` depends on no sqlx, axum, redis, reqwest or utoipa | clean |
| `application` depends on no infrastructure crate | clean |
| `api` does not depend on `infrastructure` | clean |
| Brace and paren balance across all 46 Rust files | clean |

Remaining ports without an external caller are intentional: `find_by_client_id` is used
inside the message repository's own idempotency path, `revoke_session` is superseded by
`revoke_all_for_device`, and `mentioned_users` is a read path for a mention-filter
feature that is not yet built.

---

## Still not built

Tables exist for all of these; endpoints do not. None is a defect — each is scope that
was never started.

| Area | Missing |
|---|---|
| Media | Upload sessions, presigned URLs, orphan sweeper, virus scan hook |
| Communities & channels | All endpoints (`communities`, `community_members`, `channel_followers`) |
| Status | Post, view, audience, 24-hour reaper |
| Calls | Signalling routes; an SFU such as LiveKit is a separate service |
| Moderation | Reports, admin users, moderation actions, admin audit endpoints |
| Passkeys | `passkey_credentials` table only; no WebAuthn ceremony |
| Device linking | `device_link_requests` table only; no QR pairing flow |
| Backups | `backup_metadata` table only |
| Workers | Outbox relay, disappearing-message reaper, status expiry, stale-upload sweeper |
| Message extras | Pins, bookmarks, per-message group receipts, forwarding, polls |
| Compliance | Account deletion / data export (NDPR, GDPR) |
| Tests | Integration tests against a live database; only unit tests exist |

---

## Known limitations worth a decision

**Registration is not atomic across repositories.** `verify_otp` consumes the OTP, then
creates the user, device and session through four separate repository calls. If one
fails midway, the code is spent and the user must request another. This is the cost of
one-repository-per-aggregate: a transaction cannot span them. Acceptable at this scale,
and the fix — a unit-of-work port — is worth doing before launch if it shows up in
practice.

**Logout does not kill the access token.** Sessions are revoked, but the current access
token stays valid until it expires, at most 15 minutes. That is the price of stateless
verification. A Redis deny-list keyed on `jti` is the upgrade path if instant revocation
is ever required.

**Realtime delivery is at-most-once.** Redis Pub/Sub drops events for a briefly
disconnected instance. This is safe *only because* clients re-sync from `seq` on
reconnect — which makes that client behaviour mandatory, not optional. Redpanda removes
the caveat when it is worth the operational cost.

**"Contacts only" group-invite privacy resolves as permitted.** Enforcing it
server-side needs a contact graph, which is exactly what hashed contact discovery avoids
building. The client shows the invite for confirmation instead.

**None of this has been compiled.** No Rust toolchain was available in the environment
where it was written. The checks above are structural — parameter contiguity, route and
schema coverage, dependency direction, import resolution — not a substitute for
`cargo check`. Run `make check` first.
