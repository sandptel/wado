# issues.md — reported and unfixed

Anomalies that have been **observed and attributed** but not fixed. An entry leaves this file
when the fix ships *and* someone has seen it work — not when a commit lands.

Fixed things belong in `CHANGELOG.md`. Design decisions belong in `WADO_PLAN.md`'s Decision Log.
What was measured and how belongs in `plan/` (local).

Last updated: `2026-09-12`

---

## I1 · No adaptive response to a receiver that cannot keep up · **open, highest value**

**Observed** four times on `2026-09-12`, most starkly at 16:00: the server sent 90 fps into a
decoder managing 15, for 86 seconds, and never noticed. Server metrics were perfect throughout —
render 90.0/90, pump p99 0.4 ms, zero loss. The client measured `dec` at 34–113 ms against an
11.1 ms budget and `framesDropped` climbing 60–90 per second, and only *displayed* them.

**Side:** nothing is broken; the feature does not exist. The stream is open-loop with respect to
the receiver.

**Shape of the fix.** The client already computes the verdict (`js/health.js`) and the relay data
channel already carries client → server messages, so this is one protocol message, one match arm
in `relay_client`, and an encoder rebuild the compositor already knows how to do on a settings
change.

**The hysteresis question is answered.** It was the reason this was deferred — a backoff that
oscillates is worse than none. `js/health.js` now ships a working precedent: a verdict must hold
`SETTLE_TICKS = 3` consecutive one-second samples before it is acted on. Reuse it; a decode time
sitting on its budget crosses the threshold every tick, which is exactly the oscillation a naive
backoff would turn into a bitrate sawtooth.

**Open sub-question:** does it back off *fps* or *bitrate*? The measurement below says fps.

---

## I2 · A reconnect can establish WebRTC with no session behind it · **open**

**Observed** `2026-09-12` 16:00:21. After a session stopped, the viewer reconnected: ICE reached
`connected`, `viewer connected via WebRTC` was logged, the data channel carried pings, and every
input was discarded with `input dropped — no active session`. A touch at 16:00:24 went nowhere.

**From the viewer's side** this is a frozen picture and dead taps — it reads as a hang and is not
one. `scripts/watch.sh` now flags it as `✖ ORPHAN`, which is detection, not a fix.

**Unresolved and deliberately not guessed at:** should a reconnect with no session *start* one,
or say plainly that there is nothing to attach to? It belongs with the rejoin work (`f599da4`),
which already had to answer the mirror-image question.

---

## I3 · `zwp_text_input_v3` — the focus-loss path has never run · **open, untested**

Landed `0c76378`. `TextInputs::focus_changed` sends `leave` and resets the per-object
`pending`/`enabled` state when keyboard focus moves away. **That branch has never executed**:
the session runs exactly one application, and it is not confirmed that the application binds the
protocol at all.

`scripts/osk-check.mjs` covers the client's open/close state machine (5/5). Nothing covers the
compositor side, and nothing will until a session runs two windows with a text field in one.

**Do not report the protocol as working** on the strength of the client check.

---

## I4 · ~~Chromium may never bind the protocol without a launch flag~~ · **WITHDRAWN**

**Refuted by measurement, `2026-09-12` 17:18:19**, twelve minutes after it was written:

```
11:48:19.472  launched session application command=".../google-chrome-stable"
11:48:19.693  a client bound zwp_text_input_manager_v3
```

Chrome bound the global **220 ms after launch, with no `--enable-wayland-ime` flag**. The
hypothesis was stated as a hypothesis and the discriminator was built rather than the fix — which
is the only reason this cost twelve minutes instead of a wasted launch-flag change.

**The question it raised is therefore closed too:** wado does *not* need to inject
`--enable-wayland-ime` when launching a Chromium binary. Do not add it.

**What is still open** is the next link in the chain: binding the manager is not the same as
*using* it. Chrome must send `enable` + `commit` when a text field takes focus for the keyboard to
rise, and no `text input focus changed` line has been seen yet. That is the remaining unknown, and
the same log answers it.

---

## I5 · A too-small link is only detectable indirectly · **open, by design for now**

`availableIncomingBitrate` was removed from the verdict in `1315cf9` because it lies in both
directions — it reported **123 kbps while 9.4 Mbps was flowing**, zero loss, 60/60 fps, and it
reports the *received* rate on an idle screen. It remains on the strip as a figure with no vote.

**Consequence:** a link genuinely too small is now inferred from loss or a frame-rate shortfall,
which are lagging indicators — the stream has already degraded by the time they appear. A
leading indicator would need sender-side congestion signal (the pump queue depth and the
shedding state are the candidates) pushed to the client. Not built.

---

## I8 · The playout buffer ratchets and never drains · **open, one-tap fix exists but is manual**

**Observed** `2026-09-12` 17:10: `jbuf` **1081 ms** while `rtt` was **84 ms** and `jtarget` was
36 ms. A second of felt lag on a path that had already recovered. This is the single largest
perceived-latency term measured in the whole roaming run, and `rtt` cannot see it — a viewer
saying "it feels a second behind" and a readout saying "84 ms ping" are both correct.

The buffer inflates on an rtt spike to absorb jitter and does not come back down. ⟳ Resync fixes
it in one tap by rebuilding the peer connection, which is the only thing that resets it.

**Why this is an issue and not just a feature request:** the condition is *specific*, *already
measured every second on the client*, and *one-tap fixable* — `jbuf` far above `jtarget` while
loss is low. The verdict strip already names faults and suggests settings; this is the one case
where it could offer the **action**. Today the user has to know that ⟳ exists and what it does.

See `plan/reports/2026-09-12-latency-roaming.md` for the full breakdown.

---

## I6 · The quick-tunnel URL is the rig's weakest link · **open, known**

`DEFAULT_RELAY` in `crates/client/src/state.rs` is a cloudflare quick-tunnel URL that changes
every time `cloudflared` restarts, and the one compiled into the deployed client is dead. The
field *is* persisted to `localStorage`, so it costs one paste per device — but a fresh device
gets a confusing failure ("relay reachable but no daemon answered") that reads as a daemon fault.

Baking a live URL only moves the staleness. A named tunnel, or discovery, is the real answer.

---

## I7 · ICE has no TURN, so CGNAT-to-CGNAT cannot connect · **open, architectural**

Recorded in full in `plan/memory/shared/environment.md`. Both peers behind carrier-grade NAT
produce `(host,srflx)` candidates only, ICE never leaves `checking`, and the watchdog reaps the
session after 45 s. The architecture's stated fallback — the rendezvous relay forwarding media
when ICE fails — is not built.

Not hit during the `2026-09-12` roaming run because the host has a routable public address.
