# Backend Runbook — Windows

Start to finish. Every command below is **PowerShell**.

> **Open PowerShell, not Command Prompt.** In a `cmd` window the prompt looks
> like `C:\...>`; in PowerShell it looks like `PS C:\...>`. Commands such as
> `Copy-Item` and anything starting with `$` only exist in PowerShell, and in
> `cmd` they fail with *"is not recognized as an internal or external
> command"*. If you are already in `cmd`, just type:
>
> ```
> powershell
> ```

The `Makefile` in this repo assumes `make`, which Windows does not ship. Every
step below is written out longhand instead, so you never need it.

---

## 0. Before anything — the toolchain

The error you hit:

```
feature `edition2024` is required
... not stabilized in this version of Cargo (1.79.0)
```

`rust-toolchain.toml` pinned Rust **1.79** (June 2024). Crates in the dependency
tree are now published using edition 2024, which needs **1.85 or newer** —
Cargo 1.79 cannot even parse their manifests. This is fixed in the repo
(`channel = "stable"`), so:

```powershell
rustup update stable
rustc --version        # expect 1.85.0 or newer
```

If `rustc --version` still reports 1.79, the old pin is still on disk — confirm
`rust-toolchain.toml` says `channel = "stable"`.

---

## 1. Prerequisites

| | Check | If missing |
|---|---|---|
| Rust 1.85+ | `rustc --version` | https://rustup.rs |
| Docker Desktop | `docker --version` | Only needed for local Postgres + Redis |
| Git | `git --version` | Optional |

**No Docker?** Skip to *Appendix A — Supabase* and use hosted Postgres instead.
You will still need a Redis; Upstash has a free tier.

---

## 2. Start Postgres and Redis

```powershell
cd "C:\Users\INNOV8HUB26032026\Desktop\Active Projects\NigChat\nigchat-backend"

docker compose up -d postgres redis
docker compose ps
```

Both should show `healthy`. Wait for it — Postgres reports `starting` for a few
seconds first.

```powershell
docker compose exec postgres pg_isready -U nigchat
```

Expect `accepting connections`.

---

## 3. Configuration

```powershell
Copy-Item .env.example .env
```

Now generate two real secrets. The server **refuses to boot** with the sample
values, and refuses again if the two are identical — they protect different
things and must not be the same value.

```powershell
$jwt    = -join ((1..32) | ForEach-Object { '{0:x2}' -f (Get-Random -Max 256) })
$pepper = -join ((1..32) | ForEach-Object { '{0:x2}' -f (Get-Random -Max 256) })

(Get-Content .env) `
  -replace '^JWT_SECRET=.*',  "JWT_SECRET=$jwt" `
  -replace '^HASH_PEPPER=.*', "HASH_PEPPER=$pepper" |
  Set-Content .env
```

Confirm:

```powershell
Select-String -Path .env -Pattern 'JWT_SECRET|HASH_PEPPER|DATABASE_URL|REDIS_URL'
```

Leave `OTP_DEBUG_ECHO=true` for now — it returns the verification code in the
API response so you can sign in without an SMS provider. The server will not
start with it enabled unless `ENVIRONMENT=development`.

---

## 4. Compile

```powershell
cargo check
```

The first compile of this codebase surfaced six errors. All are fixed in this
version:

| Error | Cause |
|---|---|
| `Http::builder()` not found | utoipa 4.x exposes `Http::new(scheme)`; the fluent builder arrived in 5.x |
| `type mismatch` on `Option::map(Id::as_uuid)` ×5 | `as_uuid` took `&self`, but `Option::map` hands over an owned value. Now takes `self` — the ids are `Copy`, so this is the correct signature and fixes every site at once |
| `http2_prior_knowledge` not found | `reqwest` was built with `default-features = false`, which drops HTTP/2. APNs speaks HTTP/2 only, so the `http2` feature is now enabled |
| deprecated `get_async_connection` | Only a warning. A multiplexed connection cannot enter pub/sub mode, and the replacement helper landed in redis 0.27. Marked `#[allow(deprecated)]` with a note to revisit on upgrade |
| unused imports | Removed |

If new errors appear, send them — the compiler names the exact line.

Once it is clean:

```powershell
cargo clippy --workspace --all-targets
cargo test --workspace          # unit tests only, no database needed
```

---

## 5. Run — migrations apply themselves

```powershell
cargo run -p nigchat-server
```

**Before you do — make sure `.env` points at the port Postgres is actually
published on.** They can drift apart:

```powershell
docker compose ps                                    # look at the PORTS column
Select-String -Path .env -Pattern 'DATABASE_URL'
```

If `docker compose ps` shows `0.0.0.0:5433->5432/tcp`, `DATABASE_URL` must say
`localhost:5433`. If it shows `0.0.0.0:5432->5432/tcp`, it must say
`localhost:5432`. A mismatch produces *connection refused* at startup.

**There is no separate migration command.** On boot the server runs every
migration in `migrations/` via `sqlx::migrate!`, inside an advisory lock — so
you can start ten instances at once and exactly one applies them while the rest
wait.

You should see:

```
INFO nigchat_server: starting nigchat-server environment=development
INFO nigchat_server: migrations applied
INFO nigchat_api: subscribed to the realtime event bus
INFO nigchat_server: listening addr=0.0.0.0:8080
INFO nigchat_server: API documentation at http://0.0.0.0:8080/docs
```

`migrations applied` is the line that confirms the schema is in place.

---

## 6. Verify

> **`cargo check` does not start anything.** It only type-checks. If `curl`
> cannot connect and `\dt` reports *"Did not find any relations"*, it is
> almost always because step 5 was skipped — the server has never run, so the
> migrations have never been applied. Leave `cargo run` running in its own
> window and open a second one for these commands.

> **Browse `localhost`, not `0.0.0.0`.** The startup log shows the *bind*
> address, and `0.0.0.0` means "every interface" — a browser cannot open it and
> returns `ERR_ADDRESS_INVALID`. Use **http://localhost:8080/docs**.

```powershell
curl.exe http://localhost:8080/healthz
curl.exe http://localhost:8080/readyz
```

`/healthz` proves the process is alive. `/readyz` proves it can reach Postgres
and Redis, and reports both:

```json
{ "ready": true, "postgres": true, "redis": true, "local_websockets": 0 }
```

If `ready` is false, one of the two datastores is down — the fields tell you
which.

Then open **http://localhost:8080/docs** for the full API, generated from the
handlers themselves.

The `WARN` lines at startup are expected in development:

```
WARN no SMS provider configured; codes will not be delivered
WARN FCM not configured; Android push disabled
WARN APNs not configured; iOS push disabled
```

None of them blocks anything. With `OTP_DEBUG_ECHO=true` the verification code
comes back in the API response instead of by SMS, and push is optional — a
message still sends and still arrives over the WebSocket without it.

Confirm the schema landed:

```powershell
docker compose exec postgres psql -U nigchat -d nigchat -c "\dt"
docker compose exec postgres psql -U nigchat -d nigchat -c "SELECT id, display_name FROM notification_tones ORDER BY category;"
```

You should see ~40 tables and 12 seeded notification tones.

---

## 7. Smoke test the auth flow

There is a script for this. From a **PowerShell** window, with the server
running in another:

```powershell
powershell -ExecutionPolicy Bypass -File smoke-test.ps1
```

It walks ten checks — health, readiness, OTP, registration, profile, seeded
tones, preferences, conversations, token rotation, and that rate limiting
actually refuses a second code — then prints a token pair you can paste into
the web client. It uses a random phone number each run so the per-number limit
never blocks a repeat.

> These are PowerShell commands. In Command Prompt (`C:\...>` rather than
> `PS C:\...>`) every one fails with *"is not recognized as an internal or
> external command"*. Type `powershell` first.

Or by hand:

```powershell
# 1. Request a code. With OTP_DEBUG_ECHO=true it comes back in the response.
$otp = Invoke-RestMethod -Method Post -Uri http://localhost:8080/v1/auth/request-otp `
  -ContentType 'application/json' `
  -Body '{"phone_e164":"+2348012345678"}'
$otp

# 2. Verify it and get a token pair.
$body = @{
  phone_e164   = '+2348012345678'
  code         = $otp.debug_code
  display_name = 'Test User'
  platform     = 'android'
} | ConvertTo-Json

$auth = Invoke-RestMethod -Method Post -Uri http://localhost:8080/v1/auth/verify-otp `
  -ContentType 'application/json' -Body $body
$auth

# 3. Use the token.
Invoke-RestMethod -Uri http://localhost:8080/v1/me `
  -Headers @{ Authorization = "Bearer $($auth.access_token)" }
```

If step 3 returns your profile, the whole stack works: HTTP, Postgres, Redis
rate limiting, JWT issuing and the session store.

Save `$auth.access_token` and `$auth.refresh_token` — you can paste them into
the web client's `localStorage` to exercise its UI before device pairing is
built (see the web README).

---

## 8. Point the mobile app at it

The app needs your machine's **LAN address**, not `localhost` — on a phone,
`localhost` is the phone.

```powershell
ipconfig | Select-String IPv4
```

Put that in `nigchat-app\.env`:

```
EXPO_PUBLIC_API_URL=http://192.168.1.62:8080
EXPO_PUBLIC_WS_URL=ws://192.168.1.62:8080
```

Then allow the port through the firewall — **this is the usual reason a phone
cannot reach a working server**. Run as Administrator, once:

```powershell
New-NetFirewallRule -DisplayName "NigChat API 8080" -Direction Inbound `
  -Protocol TCP -LocalPort 8080 -Action Allow -Profile Private
```

Verify from the phone's browser: `http://192.168.1.62:8080/healthz`.

---

## 9. Stopping and resetting

```powershell
docker compose stop                 # keep the data
docker compose down                 # remove containers, keep volumes
docker compose down -v              # DESTROY the database, start clean
```

After `down -v`, the next `cargo run` re-applies every migration to an empty
database.

---

## Troubleshooting

| Symptom | Cause | Fix |
|---|---|---|
| `feature edition2024 is required` | Rust older than 1.85 | `rustup update stable` |
| `could not connect to PostgreSQL` | Container not up yet | `docker compose ps`, wait for `healthy` |
| `JWT_SECRET must be at least 32 characters` | Step 3 skipped | Generate the secrets |
| `JWT_SECRET and HASH_PEPPER must be different` | Both replaced with one value | Generate two |
| `refusing to start with the sample secrets` | `.env` still has `change_me…` | Generate real ones |
| `OTP_DEBUG_ECHO must be false outside development` | `ENVIRONMENT` is not `development` | Set one or the other |
| `address already in use` | Port 8080 taken | `netstat -ano \| findstr :8080`, then `taskkill /PID <pid> /F` |
| `Bind for 0.0.0.0:5432 failed: port is already allocated` | A Postgres is already running locally | Compose now uses host port **5433**; `docker compose down` then up again |
| `'Copy-Item' is not recognized` | You are in Command Prompt | Type `powershell` first |
| `curl: (7) Failed to connect to localhost:8080` | The server is not running | `cargo run -p nigchat-server` — `cargo check` only type-checks |
| `Did not find any relations` | Same cause — migrations run on server start | Start the server, look for `migrations applied` |
| `ERR_ADDRESS_INVALID` on `0.0.0.0:8080` | That is a bind address, not a browsable one | Use `http://localhost:8080` |
| `password authentication failed` / `connection refused` on 5432 | `.env` port does not match the published port | Compare `DATABASE_URL` in `.env` with the `PORTS` column of `docker compose ps` |
| `future-incompat` warning for `sqlx-postgres` | Advance notice for a future compiler, not an error | Ignore for now; it clears when sqlx is upgraded |
| `/docs` renders blank, DevTools shows `(blocked:csp)` | The API's strict `default-src 'none'` also applied to Swagger UI's own assets | Fixed — CSP is now chosen per path, strict for the API and permissive only under `/docs` |
| Phone cannot reach the API | Firewall | Step 8 |
| `rate_limited` on repeat sign-in | Working as designed — 1 code/min, 5/hour per number | Wait, or `docker compose restart redis` to clear counters |

---

## Appendix A — Supabase instead of local Postgres

Fine for now. It is plain Postgres and the migrations run unchanged.

1. Create a project, then **Settings → Database → Connection string → URI**.
2. Use the **direct connection on port 5432**, *not* the pooler on 6543.
   Supavisor's transaction mode breaks prepared statements, which sqlx relies on
   heavily.
3. In `.env`:

```
DATABASE_URL=postgresql://postgres:YOUR_PASSWORD@db.YOUR_REF.supabase.co:5432/postgres?sslmode=require
DATABASE_MAX_CONNECTIONS=10
```

Keep the connection count low — hosted tiers cap connections far below a local
box, and the pool will happily exhaust them.

4. You still need Redis: presence, rate limiting and cross-instance fan-out all
   depend on it. `docker compose up -d redis` locally, or a hosted one.

Migrating off later is a `pg_dump` and a restore, because nothing here uses
Supabase as anything other than a database.

---

## Appendix B — Running migrations by hand

Not required — the server applies them on boot. Useful when you want to inspect
or roll forward without starting the app.

```powershell
cargo install sqlx-cli --no-default-features --features rustls,postgres

$env:DATABASE_URL = "postgres://nigchat:nigchat@localhost:5432/nigchat"
sqlx migrate info
sqlx migrate run
```

There is one migration file, `migrations/0001_nigchat_init.sql`, containing the
entire schema. It is not idempotent by design — it is the initial schema, and
sqlx tracks what has been applied in `_sqlx_migrations`.

To apply it with plain `psql` instead:

```powershell
docker compose exec -T postgres psql -U nigchat -d nigchat -f /dev/stdin < migrations\0001_nigchat_init.sql
```

---

## Appendix C — Two instances behind a load balancer

The setup that proves the architecture: a message sent to instance A reaching a
socket held by instance B.

```powershell
docker compose up --build -d
docker compose logs -f api-1 api-2
```

nginx listens on 8080 and round-robins across both. Sign in on the phone, open
the web client, and send from one — it should appear on the other instantly. If
it does not, the event bus subscription is the thing to look at.
