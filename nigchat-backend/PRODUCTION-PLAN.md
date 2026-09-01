# Getting NigChat to Production

Written after your "people should start using it this week" message. I want to be
useful rather than agreeable, so this starts with the part you will not like.

---

## The timeline

**One week is not achievable for what you listed, and it is not close.**

Here is the same list with honest estimates, assuming one experienced engineer
working full time:

| What you asked for | Realistic effort |
|---|---|
| Real OTP delivery | **half a day** |
| Supabase as the production database | **half a day** |
| Deploy backend + web + a domain | **2–3 days** |
| Contacts sync | **3–4 days** |
| Media: photos, video, documents, gallery | **2–3 weeks** |
| Voice notes | **1 week** |
| Offline-first (a local database, an outbox, conflict handling) | **3–4 weeks** |
| HD voice and video calls | **6–10 weeks** |
| End-to-end encryption, done properly | **6–8 weeks** |
| App Store + Play Store review | **1–3 weeks of waiting**, and it is not up to you |

Even the cheap items add to roughly four days. The list as a whole is four to six
months. Anyone who tells you otherwise is either not counting the same work or
is planning to ship something that loses messages.

**What you *can* have this week** is a genuine, useful product: text messaging,
groups, real sign-in over SMS, push notifications, the web client, deployed on a
real domain, with real users. That is not a demo. WhatsApp launched in 2009 with
less than that.

I would ship that, get fifty people using it, and let what they complain about
decide what gets built in week two. Building media, calls, offline and E2EE
before anyone has sent a message is how projects die at 90% complete.

---

## This week — the actual plan

### Day 1: SMS

**Use [Termii](https://termii.com).** Nigerian company, Nigerian routes, and the
delivery rates to Nigerian networks are materially better than the global
providers — this is the thing that decides whether people can sign up at all.
They give free credits on signup, enough for hundreds of test messages. Pricing
is a few naira per SMS after that.

The backend already speaks to it. Set three variables and restart:

```
SMS_ENDPOINT=https://api.ng.termii.com/api/sms/send
SMS_API_KEY=your_key
SMS_SENDER_ID=NigChat
OTP_DEBUG_ECHO=false
```

Two things to know:

- A **sender ID must be registered and approved** before it can be used, which
  takes a day or two. Until then send from their generic sender.
- Use the **`dnd` channel**, which the code already does. Most Nigerian numbers
  are on the do-not-disturb list, and the plain channel silently drops to them.
  This is the single most common reason "the code never arrives".

Alternative: [Africa's Talking](https://africastalking.com) — good coverage across
East and West Africa, better if you expand beyond Nigeria. Twilio works
everywhere but costs several times more per message to Nigerian networks and its
delivery is worse. Do not use it for this market.

### Day 1: Supabase

Fine for now, and migrating off later is a `pg_dump`.

1. New project. Pick the region with the lowest latency from Lagos — **measure
   it**, do not assume. West African traffic often routes north via subsea
   cable, so London or Frankfurt frequently beats Cape Town.
2. **Settings → Database → Connection string → URI.** Use the **direct
   connection on port 5432**, not the pooler on 6543. Supavisor's transaction
   mode breaks prepared statements and sqlx depends on them.
3. Set in the backend environment:

```
DATABASE_URL=postgresql://postgres:PASSWORD@db.YOUR_REF.supabase.co:5432/postgres?sslmode=require
DATABASE_MAX_CONNECTIONS=10
```

Keep the connection count low. Hosted tiers cap connections well below a local
box, and the pool will exhaust them.

4. **Migrations run themselves** on first boot. Nothing to do.
5. Turn off Supabase Auth, Storage and Realtime in the dashboard. You are not
   using them and leaving them on is unnecessary surface.
6. **You still need Redis** — presence, rate limiting and cross-instance fan-out
   all depend on it. [Upstash](https://upstash.com) has a free tier and is a
   two-minute setup.

### Day 2–3: Deployment

**Render for the backend.** It is the right pick here: native Rust builds,
WebSockets on every tier, health checks, zero-downtime deploys, and a
`render.yaml` you commit. It is not the cheapest or the fastest, but it is the
one with the fewest ways to go wrong, and that matters more right now.

`render.yaml` is in this repo. Set the secrets in the dashboard, never in the
file.

One thing to get right: **Render's free tier sleeps after inactivity.** For a
messaging app that means every WebSocket drops and the first request after a
sleep takes thirty seconds. Use the paid Starter tier from day one. This is not
optional for a chat product.

Alternatives worth knowing: **Fly.io** puts you closer to Lagos (a Johannesburg
region) and is cheaper, at the cost of more operational work. **Railway** is
simpler than both but has had reliability wobbles. Stay off Heroku.

**Vercel or Netlify for the web.** It is a static SPA — drag the `dist` folder
in and point your domain at it. Set `VITE_API_URL` to the Render URL, and add
that origin to `CORS_ALLOWED_ORIGINS` on the backend.

**The app is the long pole.** Play Store review is typically a few days. App
Store is one to three weeks and rejections are common for a first submission
from a new account. Start the review process *now*, in parallel — waiting until
the code is finished wastes the calendar time.

---

## What each remaining feature actually requires

So the estimates above are not just numbers.

### Voice and video calls, "HD and clearer than WhatsApp"

The backend stores call metadata. It does not carry audio or video, and it never
should — media through your API server would be catastrophic for cost and
latency.

Real calling needs:

- **An SFU** (selective forwarding unit). [LiveKit](https://livekit.io) is the
  right choice — open source, self-hostable, and a managed cloud if you prefer.
- **TURN servers**, because roughly a fifth of connections cannot go
  peer-to-peer behind carrier NAT. Nigerian mobile networks are worse than that
  average.
- **Client integration** on iOS, Android and web, plus CallKit and
  ConnectionService so calls appear as real calls on the lock screen.
- **VoIP push** so a locked phone rings at all.

On quality specifically: "better than WhatsApp" is a bandwidth and codec
problem, not a code problem. WhatsApp spends enormous effort on low-bitrate
performance because that is what most of the world has. Opus for audio and
VP9/AV1 for video with proper simulcast will get you comparable quality on good
connections. Beating them on a 3G connection in Lagos is a multi-year research
effort, not a sprint. I would target *reliable* first and *HD* second — a call
that connects every time at decent quality beats one that is beautiful when it
works.

**6–10 weeks.** Budget for TURN bandwidth; it is a real running cost.

### Media

Presigned uploads to S3-compatible storage — the schema is ready, the endpoints
are not. Then client-side encryption of each file, thumbnail generation, video
transcoding for the range of devices, progressive download, a gallery picker per
platform, document preview, and the retry logic that makes a 40 MB upload
survive a train tunnel.

**2–3 weeks**, and it is where most of your storage bill will come from.

### View-once messages

Straightforward server-side (a flag and a reaper). The hard part is that you
cannot actually prevent a screenshot. Signal and WhatsApp both detect and notify
rather than block. Be careful how you word it to users — promising something you
cannot enforce is worse than not offering it.

**3–4 days.**

### Offline support

The largest item on the list and the least visible. It means a real local
database (SQLite or MMKV), a send queue that survives being force-quit,
reconciling optimistic state against the server's `seq`, conflict handling for
edits and deletes, and background sync on both platforms.

The current app keeps messages in memory. Close it and they are gone until the
next fetch. Making it genuinely offline-first is **3–4 weeks** and touches
almost every screen.

### End-to-end encryption

The wire format carries ciphertext today, but nothing encrypts it — messages are
base64, which is encoding, not encryption. Doing this properly means the Signal
protocol via `libsignal`, session state per device pair, key rotation, safety
number verification, and encrypted backups.

**6–8 weeks**, and it must be done before you claim it in the marketing copy.
The onboarding screen currently says "Every message is encrypted on your device."
**That is not true yet.** Either build it or change the text — shipping that
claim as it stands is the kind of thing regulators and journalists notice.

---

## What I would do

**Week 1:** SMS, Supabase, deploy, fix the encryption claim in the copy, put it
in front of fifty people.

**Week 2–3:** Contacts sync and photo sharing. These are what users will ask for
first, in that order.

**Week 4–6:** Offline support. By now you will have complaints about lost
messages on the Lagos underground, and this fixes them.

**Month 2–3:** Voice notes, then calls.

**Month 3–4:** E2EE, and only then make the privacy claim.

Ship, listen, then build. In that order.
