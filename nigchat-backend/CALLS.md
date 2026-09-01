# Calls — what exists and what is left

I told you calls were six to ten weeks. That was wrong as stated, and I want to
be precise instead.

**Six to ten weeks was for production-grade calling** — CallKit so it rings on a
locked iPhone, VoIP push, group calls, reconnection when someone walks into a
lift, quality tuning for 3G. That is the polish.

**A working one-to-one audio or video call is much closer than that**, because
LiveKit does the hard part. The whole backend half is now built.

---

## Built

| | |
|---|---|
| `POST /v1/calls` | Start a call. Rings everyone else over the socket and by push, returns a media-server token. |
| `POST /v1/calls/{id}/join` | Answer. Returns a token for the same room. |
| `POST /v1/calls/{id}/end` | Hang up, decline, or leave a group call. |
| `GET /v1/calls` | History. |

Enforced along the way, because a call is a more intrusive thing than a message:

- **Blocks work both ways.** A blocked user cannot ring you and you cannot ring
  them.
- **"Who can call me" is honoured.** Set to nobody and calls are refused before
  anything is created.
- **The participant list is the guest list.** Knowing a call id is not enough to
  join; you have to have been rung.
- **Tokens are scoped to one room** and expire in ten minutes. A leaked token is
  useless elsewhere and useless soon.
- **Rate limited** to 30 starts an hour. Ringing people is intrusive and cheap
  to abuse.
- Push already treats calls as high priority and lets them through quiet hours.

---

## Setup — about ten minutes

1. **livekit.io → Cloud → create a project.** The free tier is enough to start
   and includes the TURN servers, which is the part you would otherwise have to
   run yourself.

2. Copy the URL, API key and secret into the backend:

```
LIVEKIT_URL=wss://your-project.livekit.cloud
LIVEKIT_API_KEY=API...
LIVEKIT_API_SECRET=...
```

3. Restart. Look for `calling enabled` in the logs. Without those three the call
   endpoints refuse cleanly and the rest of the app is unaffected.

At this point calls work from the API's point of view. You can start one and get
a valid token back.

---

## What is left — the client

The app needs LiveKit's SDK:

```bash
npx expo install @livekit/react-native @livekit/react-native-webrtc
```

This requires a **development build**, not Expo Go — WebRTC is native code.

Then roughly:

1. An incoming-call screen, triggered by the `call_signal` socket event.
2. Connect to `server_url` with the `token` the API returned.
3. Render the remote video track, and the local one as a preview.
4. Mute, speaker, camera flip, hang up.

**That is days of work, not weeks.** LiveKit's React Native SDK handles the
WebRTC negotiation, and their documentation has a working call screen you can
adapt.

## What genuinely takes longer

- **CallKit and ConnectionService** — making a call ring like a real call on a
  locked phone, and appear in the system call log. A week or so, and it needs
  native configuration on both platforms.
- **VoIP push** so a fully backgrounded iPhone wakes and rings. Apple is strict
  about this: a VoIP push that does not result in a call reported to CallKit can
  cost you the entitlement.
- **Group calls beyond a few people** — LiveKit handles the forwarding, but
  simulcast and layout need tuning.
- **Quality on poor connections.** This is the real long pole and it never quite
  ends. WhatsApp spends enormous effort here because most of the world is not on
  fast networks. Aim for *reliable* first: a call that connects every time at
  decent quality beats one that is beautiful when it works.

---

## Cost

LiveKit Cloud is free for a small amount of usage, then billed per participant
minute. **TURN bandwidth is the part to watch** — roughly a fifth of connections
cannot go peer-to-peer behind carrier NAT, and Nigerian mobile networks are
worse than that average. Budget for it before you have a thousand users, not
after.
