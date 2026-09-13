# issues.md — reported and unfixed

Anomalies that have been **observed and attributed** but not fixed. An entry leaves this file
when the fix ships *and* someone has seen it work — not when a commit lands.

Fixed things belong in `CHANGELOG.md`. Design decisions belong in `WADO_PLAN.md`'s Decision Log.
What was measured and how belongs in `plan/` (local).

Last updated: `2026-09-12`

---

## I1 · No adaptive response to a receiver that cannot keep up · **fixed and seen working; one field bug found and fixed**

**The failure.** A2 of the roaming run: the server pushed 90 fps into a phone managing 15, for 86
seconds, and never noticed — every server-side metric was perfect throughout. The client measured
`dec` and `framesDropped` and only *displayed* them, so the whole loop ran through the viewer
reading a suggestion and changing a setting by hand.

**Fix.** The client's settled verdict now goes back to the daemon as `RelayMsg::ViewerStrain`, and
`congestion.rs` treats it as a second stress input beside the pump drop counter. No new mechanism:
shedding render ticks was already the only fps lever the render loop holds, because neither encoder
backend exposes a runtime bitrate change.

**The two signals move at different speeds, deliberately.**

| | Signal | Response |
|---|---|---|
| Link | pump drop delta in a window | halve immediately — by the time drops are plural the viewer has seen it |
| Phone | viewer strain, a latched level | step down after 3 consecutive strained windows |

Strain is counted rather than acted on because it arrives at 1 Hz and latches, while a decision
window is 60 *ticks* — at 90 fps and divisor 4 that closes in 0.67 s. Acting on the first strained
window would walk to the floor before the phone could measure the previous step. Strain also
blocks recovery outright, so a struggling viewer never climbs back underneath itself.

**Hysteresis, end to end:** `SETTLE_TICKS = 3` on the client verdict, then `STRAIN_WINDOWS = 3` in
the compositor. A one-second spike reaches nothing.

**A second bug fell out of it.** The client's device rule had no arrival gate, so a *starved*
decoder was being reported as an overloaded one — live at 20:36:56, `decode 48.7 ms` while
69 kbps of 5.7 Mbps was arriving, which `scripts/watch.sh` correctly called UNCLEAR about the same
sample. `TRUST_DECODER_FRAC = 0.6` now gates it, matching the monitor. Shedding on that signal
would have cut the frame rate of a stream that was not arriving in the first place.

**Attribution is preserved:** `Reason` is logged with every divisor change and the monitor prints
`the PHONE asked for it` or `the LINK`. A shed that could have come from either would have undone
the side attribution the rest of the run was spent building.

**Seen working `2026-09-12` 21:43**, and it exposed a bug the unit tests could not: the flag
**oscillated**. The compositor walked `1 -> 2 -> 1 -> 2` for a minute, one round trip roughly
every 1.5 s, with `dropped_in_window=0` throughout — so the strain path was firing end to end,
and doing the one thing `congestion.rs` says is worse than doing nothing.

```
16:13:22  shedding render ticks — the viewer says its decoder is saturated  from=1 to=2 strained=true
16:13:23  viewer reported a change in decoder strain strained=false
16:13:24  shedding render ticks — clean windows — easing back toward full rate from=2 to=1
16:13:28  viewer reported a change in decoder strain strained=true
16:13:30  shedding render ticks — the viewer says its decoder is saturated  from=1 to=2 strained=true
```

**Cause, in the client.** `reportStrain(saturated && side === "your device")` mixed *this tick's*
decode reading with the *settled* verdict. A decode time working near its budget crosses the line
every second or two, so `saturated` flipped while `side` stayed settled — reaching straight past
the `SETTLE_TICKS` hysteresis that existed two lines above to prevent exactly this.

Now `reportStrain(side === "your device")`, settled and nothing else. Sufficient on its own:
both device rules live inside the `arriving` branch, so that side cannot be reached unless the
stream was genuinely turning up.

**The test that was missing.** Every strain case fed a *constant*, and a constant cannot flap.
The new case settles the verdict and then dips under the threshold every few ticks — what a
decoder at its limit actually looks like. Against the shipped code it produces 13 flips; against
the fix, one report.

**Tests:** 6 new in `congestion.rs` (12 total), 5 new in `scripts/health-check.mjs` (12 total),
4 monitor rules replay-tested.

### The loop fed on its own output — `2026-09-12` 21:47

With the flap fixed, a slower cycle remained: `1 → 2 → 4 → 2 → 1`, about ten seconds per lap.
Not a tuning problem. Correlating every release against the divisor in force:

| | strain asserted at | after shedding to 1-in-4 | released |
|---|---|---|---|
| 16:17 | 4.8 of 5.7 Mbps arriving (85%) | 816 kbps (**14%**) | `bad the server` |
| 16:18 | 6.0 of 5.7 Mbps arriving | 1.6 Mbps (28%) | `ok healthy` |

Every release landed at divisor 4 and none at divisor 1. **The viewer measures saturation under
the mitigation**, so shedding destroys the evidence that justified shedding, and the release
condition is "the symptom went away" — which it always does. A constant recovery delay only sets
the period of that.

**Two fixes, because there were two faults.**

1. **Recovery patience grows** (`PATIENCE_FACTOR = 4`, capped at `PATIENCE_MAX = 320` windows —
   about 3.5 minutes at 90 fps). Each strain-driven step down makes the next probe back toward
   full rate rarer, so the loop converges in two or three laps instead of running forever. The
   probe itself is inherent: whether the phone can hold a higher rate is not knowable without
   trying it. The pump-drop path keeps the quick recovery — it is measured locally and is not
   affected by the mitigation.

2. **`RelayMsg::Shedding { divisor }`, server → client.** At 16:17:33 the strip read
   `bad the server — only 816 kbps arriving of 5.7 Mbps` about a frame rate the phone had asked
   for three seconds earlier — wado accusing itself, the exact failure written into
   `plan/memory/shared/verification.md` earlier the same evening. The client now scales its
   expected throughput, its decode budget and its arrival gate by the divisor. A genuinely dead
   sender is still caught underneath an active shed, and a phone still over budget *at the
   reduced rate* is still named.

Without the scaling, a comfortable phone under a 1-in-4 shed reads `bad your device — decode
20.0 ms against a 11.1 ms budget` and ratchets the shedding to the floor. Pinned by test.

**Monitor:** a strain-driven shed no longer sets `srv_bad`. It was making the next client sample
read `SERVER … this is OURS, not the link` about a shed the phone requested; there is now a
`DEVICE` verdict that says so plainly.

**Tests:** `congestion.rs` 14, `scripts/health-check.mjs` 15, monitor rules replay-tested. Two of
the new cases were checked against the *shipped* code and fail there — the flap case produces 13
flips, the shed case misattributes to the device.

## I2 · A reconnect can establish WebRTC with no session behind it · **open**

**Observed** `2026-09-12` 16:00:21. After a session stopped, the viewer reconnected: ICE reached
`connected`, `viewer connected via WebRTC` was logged, the data channel carried pings, and every
input was discarded with `input dropped — no active session`. A touch at 16:00:24 went nowhere.

**From the viewer's side** this is a frozen picture and dead taps — it reads as a hang and is not
one. `scripts/watch.sh` now flags it as `✖ ORPHAN`, which is detection, not a fix.

**Unresolved and deliberately not guessed at:** should a reconnect with no session *start* one,
or say plainly that there is nothing to attach to? It belongs with the rejoin work (`f599da4`),
which already had to answer the mirror-image question.

> **One cause found and fixed, `2026-09-12` 21:46:53.** The client sent `session_stop` on
> `pagehide` — which on a phone fires on an app switch, a pulled-down shade or a screen lock, not
> only on close. The session and every window died while the page kept its WebRTC connection and
> data channel alive, and the viewer went on swiping into it for ten seconds
> (`input dropped — no active session`).
>
> Relay mode now sends nothing on `pagehide`. `viewer_watchdog` is the replacement and is
> browser-independent; the beacon predates it. Direct mode keeps the beacon, having no watchdog
> of its own. `event.persisted` is logged rather than trusted — bfcache eligibility is revoked by
> an open WebSocket or WebRTC connection in several Chromium versions, so the flag may read
> `false` on the very app switch it is meant to identify, and the next one will say.
>
> This does not close I2 — a reconnect can still find no session by other routes — but it removes
> the one that was firing routinely.

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

## I4 · Chromium binds the text-input protocol but never enables it without `--enable-wayland-ime` · **fix landed, unverified by a human**

**History of this entry, kept because the mistake is the useful part.** It was first filed as
"Chromium may never bind the protocol without a launch flag", then marked WITHDRAWN twelve
minutes later on this evidence:

```
11:48:19.472  launched session application command=".../google-chrome-stable"
11:48:19.693  a client bound zwp_text_input_manager_v3
```

Chrome bound the global 220 ms after launch with no flag — so the *bind* claim was genuinely
refuted. **The withdrawal went too far.** Binding is not using. Chromium registers the global as
part of enumerating Wayland globals; the flag is what makes it construct a Wayland input-method
context and call `enable` on a focused text field. Every session since has logged the bind and
**never once** logged `text input focus changed`, which is exactly the signature of bound-but-
never-enabled.

**Reported by the user** `2026-09-12` 20:30: *"the input keyboard auto detect does not work"* —
the ⌨ button raises the keyboard, tapping a text field does not.

**Fix.** `headless::with_ime_flag` appends `--enable-wayland-ime` to a Chromium-family command
that lacks it, at the single launch choke point. A `.desktop` file written for a laptop has no
reason to carry the flag and the user never types the command, so nowhere else could add it.
Covered by three tests in `headless.rs`.

**Not yet confirmed working** — nobody has watched a `text input focus changed` line appear. Until
one does, the flag is a well-supported hypothesis, not a verified fix, and I3's untested
focus-loss path stays untested.

## I9 · `Reactivity` emits a keyframe every second at **7× the per-frame budget** · **open, measured**

**Measured** `2026-09-12` 17:51, live, `1080×2422 @90`, `Reactivity` (5676 kbps):

| | |
|---|---|
| CBR per-frame budget | 5676 / 90 = 63 kbit = **7 kB** |
| measured P-frames | **7 kB** — exactly on budget |
| measured keyframes | **46–51 kB** — **7× the budget** |
| cadence | 3–4 per 300 frames ⇒ **one per second** (`Reactivity` sets GOP = fps) |

Every second the stream emits one frame seven times the size of the budget, and the pump must
clear it inside a single 11 ms frame period — an instantaneous demand of roughly **36 Mbps** on a
link that was offering **1.4 Mbps**.

**Correlates with the dominant failure of the roaming run.** ICE held 3.3 s and 8.1 s on two
consecutive `Reactivity`/90 fps sessions before `disconnected`; `Balanced`/60 fps sessions earlier
the same hour ran for three and five minutes. (The `failed` state that follows is always exactly
30 s later — that is the ICE timer, not information.)

⚠️ **This is a mechanism plus a correlation, not a proof.** The sample is two sessions, the user
was moving, and a later `Reactivity`/90 session ran healthily at 89/90 fps with rtt 42 ms.

**The refuting measurement:** same link, same fps, `Balanced` (GOP = 2 × fps) against `Reactivity`
(GOP = fps). If the disconnect rate does not move, the burst is not the cause.

**It also sits against invariant 7.** `CLAUDE.md` says *"keyframes on demand rather than periodic
… on-demand IDR is how that goal is met instead"*, and the codebase has on-demand IDR (PLI/FIR →
`ForceKeyframe`). But `conf/mod.rs:130` gives `Reactivity` a **1-second periodic GOP**, and the
other tiers 2 seconds. If on-demand IDR is the mechanism, a periodic GOP an order of magnitude
longer would cost nothing in recovery and remove the burst entirely. **That is a Decision Log
question, not a patch to make quietly.**

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

> **Correction, `2026-09-12` 18:00.** That sentence was wrong for the mode this was measured in.
> `W.resync()` called `connectWebRTC()`, which POSTs to `/offer` — an endpoint that exists only
> in direct mode. In relay mode the tap threw and the buffer stayed inflated. Fixed alongside the
> reconnect work below (I12); the issue itself is unchanged, and the one-tap fix is now real.

**Why this is an issue and not just a feature request:** the condition is *specific*, *already
measured every second on the client*, and *one-tap fixable* — `jbuf` far above `jtarget` while
loss is low. The verdict strip already names faults and suggests settings; this is the one case
where it could offer the **action**. Today the user has to know that ⟳ exists and what it does.

See `plan/reports/2026-09-12-latency-roaming.md` for the full breakdown.

---

## I10 · `surface missing from known popups` — one ERROR from smithay, cause unknown · **open, low**

Seen once, `2026-09-12` 17:37:55, immediately before an `xdg_activation` request with
`app_id=None`. One occurrence in a daemon run of an hour with dozens of sessions, so it is rare
rather than harmless — the difference has not been established.

```
ERROR smithay::wayland::shell::xdg: surface missing from known popups
DEBUG wado_compositor::handlers::activation: activation request — raising and focusing app_id=None
```

It is logged by smithay, not by wado, and nothing in wado reacted to it. The plausible reading is
a popup destroyed between its map and its teardown — routine during menu churn in a browser — in
which case it is noise from upstream. **Filed rather than dismissed** because an `ERROR` that
nobody has explained is exactly the kind of thing that turns out to matter later, and one line in
this file costs nothing.

Next occurrence: note what was on screen. If it correlates with a menu or a file dialog, it is
the benign reading.

---

## I11 · A session with no window yet is indistinguishable from a broken one · **open, low**

Two of seventeen sessions this run ended with `windows=0` — the compositor never had a surface
to composite, so the viewer saw black. Both were stopped by the user within about eight seconds,
which is around how long a cold Chrome takes to map its first window here.

```
compositor session active width=720 height=1614 fps=60 …
… 8 s later, no window ever mapped …
dmabuf path unused this session — every client buffer went through wl_shm windows=0
compositor session stopped — resources released
```

**Not a fault in itself** — an application takes time to start. The gap is that nothing says so:
once WebRTC connects, the status reads as running and the picture is black, which is exactly what
a genuinely broken session looks like. The viewer's reasonable response is to stop and retry,
which restarts the cold start and can loop.

The compositor already knows `windows == 0`; saying "waiting for the application to open a
window" until the first surface maps would close it. **No process leak involved** — `ps` shows no
orphaned Chrome between sessions, so `proc::spawn`'s cleanup is doing its job.

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

---

## I12 · A viewer that gave up pins the session until the tab closes · **open, low, accepted**

Fallout from making `viewer_watchdog` the only teardown (Decision Log, `2026-09-12`). The
watchdog needs `VIEWER_GRACE` of **relay-link silence**, and the client answers the daemon's
keepalive with `{"type":"pong"}` — a WS *text* frame, so it bumps `last_relay_msg`. A tab that
is open, joined, and has exhausted `MAX_RECONNECTS` therefore holds the session open
indefinitely: it is silent on WebRTC but not on the relay.

Bounded in practice — `pagehide` sends `session_stop`, so closing the tab or the browser ends
it. The unbounded case is a tab left open on a dead connection for hours.

The clean fix is a viewer-liveness signal distinct from relay traffic (last `pong` *while*
WebRTC was connected, say), not a shorter grace. Not worth building until it is observed.

---

## I15 · The 45 s grace granted no grace — the watchdog was on the wrong clock · **fixed, measured**

**Observed** `2026-09-12` 22:34, live, on the build that had shipped three hours earlier:

```
17:04:03.875  peer connection Failed — session kept, waiting for a re-offer
17:04:06.433  no sign of a viewer for 45s and WebRTC is not connected — stopping
              the session ... silent_ms=47699
17:04:06.499  compositor session stopped — resources released
```

**2.56 seconds**, not 45. The evening's headline fix — stop tearing sessions down on a transient
peer-connection failure — was undone one layer along by the watchdog that was supposed to be its
only remaining teardown. The client's ~37 s retry budget never got a chance to run.

**Cause.** `viewer_watchdog` required relay-link silence for `VIEWER_GRACE` *and* a non-connected
peer connection. That reads as two conditions and is really one, because **a healthy viewer is
silent on the relay link**: its media and its input ride WebRTC, and it speaks to the relay only
when something changes. `silent_ms=47699` — the grace had already elapsed *before* the fault, so
the second condition flipping was the whole decision.

**Fix.** The first clock is now *how long since a viewer was last actually connected*, which is
the thing "no viewer" was always trying to measure — sampled every tick while a peer connection is
`Connected`, and reset when a session starts or is rejoined so a viewer that never arrives is
still reaped. Relay silence stays as a genuine second clock: a viewer whose WebRTC is down but who
is **re-offering through the relay right now** is present, and killing the session it is trying to
rejoin is the worst available move. Both clocks must be old.

Extracted as `should_reap(gone_ms, silent_ms, connected)` and covered by 5 tests, one of which is
the measured numbers above.

**The general shape, and it is the third time tonight:** *a proxy signal was standing in for the
thing that mattered.* Relay silence for viewer absence here; `saturated` this tick for the settled
verdict in I1; `Drop` for release in I14. Each read plausibly and measured something else.

---

## I13 · `VIEWER_GRACE` is now load-bearing at 45 s · **open, needs a real measurement**

With the immediate teardown gone, 45 s is the entire budget a viewer has to come back before its
windows and applications are destroyed. The roaming run of `2026-09-12` had dead zones longer
than that, so the number is very likely too small for the case it now governs.

Raising it trades preserved state against holding Chrome, the encoder and the GPU with nobody
watching. **That is the user's call, not a code decision** — left at 45 s until asked, with the
client's retry budget (~37 s, `scripts/reconnect-check.mjs` case 6) sized to fit inside it. Both
numbers move together or the client abandons a session the server would still have honoured.

---

## I14 · The daemon ran out of ICE ports and stopped answering with candidates · **fixed, confirmed by measurement**

**Observed** `2026-09-12`, after ~2.5 hours of reconnect churn. The daemon's ICE answers decayed:
`8 candidates (host,srflx)` all afternoon, then at 18:19:25 `2 candidates (host)`, then from
18:19:41 onward **`0 candidates (none)`** on every negotiation. The client's offer was still a
healthy 8. No client could connect, and the user reported it as *"connection can not be
established past relay"*. A daemon restart fixed it instantly.

**Mechanism.** `webrtc_settings.rs` pins ICE to **101 UDP ports** (`EphemeralUDP::new(50000,
50100)`) so a host firewall can open exactly that range. `relay_client.rs` replaced
`ctx.active_pc` on every re-offer and **dropped** the old `RTCPeerConnection` — but dropping one
frees nothing in webrtc-rs: the ICE agent, its gathering tasks and its bound sockets sit behind
internal `Arc`s and are released only by `close().await`. Every negotiation therefore leaked its
sockets. Measured on the fresh daemon: 4 ports in the range after one negotiation, **8 after two**, with
nothing released in between — so the pool is gone after roughly 25 negotiations, which the run
passed well before 18:19.

That also explains the shape of the decay. Port exhaustion is gradual, so gathering first loses
the srflx candidates (fewer sockets to probe from), then the host ones, then all of them. It is
not a STUN failure, despite the log line saying so.

**Not fd exhaustion** — the daemon's limit is 524288 and it was holding 20. The narrow *port*
range is the whole constraint.

**Fixed** by closing the previous peer connection when it is replaced, spawned rather than
awaited so the new answer does not queue behind the old connection's shutdown. **Verified on the
live daemon**: two negotiations, still 4 ports, and an `ICE closed` line for the old connection —
before the fix the same two negotiations read 8. This became
urgent rather than tidy with the same change: re-offers are now the normal recovery path, so the
leak would have been hit in minutes instead of hours.

**Still worth adding** — nothing warns as the pool drains. An answer with zero candidates is
reported as "STUN timed out", which accuses the network for a local resource leak. A count of
bound ports in the range, logged when an answer carries fewer candidates than the last one, would
have named this in one line.

---

## I16 — kitty exits when the session's output is replaced (Chrome does not)

Found `2026-09-13` by `scripts/graceful-probe.mjs` while verifying live reconfigure.

A resize replaces the session's `Output` — it has to, because Wayland cannot un-advertise a
mode (invariant #8). Within ~260 ms of that, a `kitty` running in the session closes its display
connection and exits, taking its child processes with it.

**It is not the compositor killing it.** `ClientData::disconnected` now reports the reason
(that function was previously empty, which is why this took a cycle to establish) and it says
`ConnectionClosed`, not `ProtocolError`. kitty is choosing to exit. It writes nothing to stderr.

**Scope, measured three ways on the same build:**

| application | resize |
|---|---|
| `sleep` (no Wayland client at all) | survives |
| `google-chrome-stable` — the real use case, 12 processes | **survives** |
| `kitty` | **exits** |

So the reconfigure path is not broken in general. Two things were fixed along the way and both
are worth keeping regardless: a reconfigure that does not change the output's *shape* (a bitrate
change) no longer rebuilds the output at all, and a replaced output global is now
`disable_global`'d and destroyed five seconds later rather than immediately, so clients get the
round trip `global_remove` is supposed to give them. Neither saved kitty — it was already gone
before the retire timer fired.

**Open.** Not chased further because the application wado actually runs survives, and the next
step is reading kitty's source rather than wado's. Worth revisiting if a second client turns out
to behave the same way.

## I17 — two viewers of one session fight over the peer connection

Reachable now that a page reload rejoins automatically (the `wado.watching` crumb): two tabs on
the same device, or two devices, both take the session back. Each rejoin replaces the active
peer connection and forces a keyframe, so they alternate indefinitely and neither gets a stable
stream.

The daemon already assumes a single viewer — `active_pc` is one slot, and `generation` exists to
let a stale viewer's teardown be ignored. What is missing is any *arbitration*: the second
viewer is not told it displaced anyone, and the first is not told it was displaced.

**Half fixed `2026-09-13`, after it was observed live on the user's own connection.**

The relay log made the mechanism plain. Between 11:56:43 and 11:58:17 there were **18
`join: room create failed — already has an active room`** entries from different source ports,
spacing out as the backoff grew; then the incumbent left, the knocker got in, and the room
changed hands **four times at ~18-second intervals**.

Two changes that are individually right compose into a livelock:

* `join_denied` became retryable — correct, because it usually means a daemon that is restarting.
* a reconnect auto-rejoins — correct, because the viewer never chose to leave.

Together: the loser knocks every 500 ms, takes the room the instant the incumbent's socket
blips, auto-rejoins, and kicks them. They then do the same back, forever.

**Fixed** by separating the two denials. `"no server online with this Remote ID"` is still
retried at network speed, forever. `"server already has an active connection"` backs off to the
15 s cap, and when the room does come free the session is **not** taken automatically — getting
in because somebody else left is not the same event as our own reconnect, so a human presses
Start. `scripts/relay-link-check.mjs` pins both branches and reads the relay source so the
wording it matches cannot be renamed out from under it.

**Still open:** the *displaced* viewer is told nothing — it just goes black and starts knocking.
Telling it what happened is the remaining honest minimum, and it needs a wire message the relay
does not have.

## I18 — decode time collapses after a long reconnect gap (unattributed)

Observed `2026-09-13 12:02` on the user's live session, with the server clean throughout
(render pacing 90.0/90, pump `avg_queue_ms=0`, `write_sample` p99 0.1 ms, `lost=0`).

| time | fps | kbps | rtt | dec | framesDropped |
|---|---|---|---|---|---|
| 12:00:13 | 37 | 2753 | 27 ms | **10.95 ms** | 4 |
| 12:01:02 | 43 | 2468 | 32 ms | 11.30 ms | 4 |
| *(ICE closed 12:01:55, re-offer 12:02:46 — a 51 s gap)* | | | | | |
| 12:02:54 | 35 | 3047 | 34 ms | **108.98 ms** | 229 |
| 12:03:09 | 42 | 2891 | 38 ms | **106.01 ms** | 947 |

The link is not the problem: rtt is flat, loss is zero, and ~3 Mbps is arriving. The phone is
receiving about 90 frames a second and dropping half of them, with each decode taking four times
the frame interval.

**Two candidates, not yet separated:**

1. **The page was backgrounded or the screen went off** — a throttled tab still receives RTP but
   decodes lazily, which produces exactly this shape. The 51 s connection gap immediately before
   is consistent with a phone that was locked.
2. **Thermal throttling** on the phone's decoder.

What would separate them: the client already knows `document.visibilityState`, and it is not in
the stats line. Adding it costs one field and turns this from a guess into a reading — the same
argument that made `ClientData::disconnected` worth filling in. **Shipped** as `vis=` on the
stats line; the next session's log answers it.

### ⚑ Narrowed the same hour, without the new field

The episode recovered **on its own**, and how it recovered is evidence:

| time | shed | fps | dec | framesDropped |
|---|---|---|---|---|
| 12:04:23 | 1 in 1 | 43 | 92.6 ms | 1335 *(climbing fast)* |
| 12:04:26 | 1 in 2 | — | — | — |
| 12:04:28 | 1 in 4 | — | — | — |
| 12:04:38 | 1 in 4 | 30 | 39.0 ms | 1623 |
| 12:04:53 | 1 in 4 | 29 | **11.5 ms** | 1623 *(stopped)* |

Decode fell 92.6 → 11.5 ms and the drops stopped, **purely because the offered frame rate was
reduced** — no reconnect, no reload, nothing the viewer did.

That argues against candidate 1. A backgrounded tab does not start decoding promptly again
because fewer frames are offered; it is throttled regardless of rate. A decoder that is simply
**past capacity at 90 fps and comfortable at ~30** behaves exactly like this.

So the leading explanation is now the plain one — this phone cannot decode 1280x720 at 90 fps —
and the congestion loop is doing its job. What is still unexplained is the *step*: 11 ms at
12:01 and 92 ms at 12:04 on the same stream and the same phone. A decoder at its limit should
degrade, not sit fine for a minute and then quadruple. The 51 s connection gap between them is
still the only other thing that changed, so `vis=` is still worth reading before this is closed.

**Open**, but no longer a mystery about *which side* — it is the phone, and the fix already
fires.


## I19 — every reconnect re-flooded the viewer at full frame rate (fixed, not yet deployed)

Found `2026-09-13 12:18` by reading the daemon log during the user's live session. A regression
introduced by this branch's own consolidation of the per-viewer reset into `ViewerAttached`.

`set_viewer_attached(true)` called `Congestion::reset()`, which restores `divisor = 1`. On a
mobile link that reconnects every one to four minutes — **exactly the link this branch exists
for** — the phone was handed the full 90 fps again on every return, saturated again, and had to
walk the divisor back down from scratch. The log shows it without ambiguity: every
`viewer attached` is followed by a fresh `shedding … from=1`.

```
06:32:47  viewer attached
06:34:26  shedding — the viewer says its decoder is saturated  from=1
06:34:28  shedding — the viewer says its decoder is saturated  from=2
...
06:47:23  viewer attached
06:48:16  shedding — the viewer says its decoder is saturated  from=1
```

**This also explains I18.** The decode spikes were not a mysterious decoder collapse — 109 ms
eight seconds after one reconnect, 55 ms thirty seconds after another. The server had just gone
back to sending four times as many frames. On a link reconnecting every couple of minutes the
phone spent much of its life in that re-saturation transient.

The original reasoning — *"a new decoder starts with no history and must not inherit a shed"* —
is right for a **new viewer** and wrong for the same viewer reconnecting forty seconds later,
whose decoder is the same silicon that could not keep up before. And a reconnect is now the
common case, which it was not when that line was written.

**Fixed** with `Congestion::reattach()`: the divisor survives, the patience does not. Patience
grows with every strain to stop the loop feeding on its own output, but carrying a long
session's accumulated patience across a reconnect would make recovery glacial for a viewer that
has genuinely improved, or for a different and faster device. Keeping the rate and giving
recovery a fresh start is wrong in neither direction. `reset()` keeps its old meaning for a
genuinely new session; three tests pin the difference.

### The client half is covered by construction — checked, not assumed

The client keeps its own copy of the divisor, and `setTargetKbps` resets it to 1. That runs on
every `session_started`, **including a rejoin**, so for a moment after a reconnect the viewer
believes it is being sent the full rate while the daemon is at 1-in-4 — which is precisely the
mis-attribution the `Shedding` message was added to prevent.

It does not bite, for two independent reasons, and both were verified rather than assumed:

- the server sends the **current** divisor on attach (`shedding_tx.send(congestion.divisor())`)
  and the forwarding task calls `rx.mark_changed()`, so an attaching viewer is told the live
  value rather than waiting for the next change;
- `setTargetKbps` also resets `warm`, and `WARMUP_TICKS` suppresses the verdict for five ticks
  — far longer than the message takes to arrive.

Worth knowing because the first of those is load-bearing: drop the `mark_changed()` and the
window opens.

⚠️ **Committed, not deployed.** Shipping it needs a daemon restart, which kills the running
session — the ceiling this branch cannot lift. Held until the user is idle.


## I20 — a hidden page was served the full stream (fixed and confirmed in the field)

Found `2026-09-13 12:34:57` by the `vis=` field shipped an hour earlier for a different question
(I18), which is the second time that field answered something it was not added for.

A backgrounded tab or a locked screen holds a live peer connection and still receives RTP; the
browser simply stops pulling frames and discards them. The daemon cannot see this — at the
transport layer a hidden page is a watched one — so it kept rendering, encoding and transmitting
the whole stream:

```
12:34:57  bad network — only 65 kbps arriving of 11.9 Mbps the server actually sent
```

**11.9 Mbps out, 65 kbps reaching the decoder.** On a phone, on mobile data.

Two things were wrong at once, and the verdict line above contains both: the waste, and the fact
that the strip blamed *the network* for it. It was right that the bytes went in and did not come
out; it had no way to know why.

**Fixed** with `RelayMsg::ViewerVisible` — the client reports `visibilitychange`, and the render
tick requires `viewer_attached && viewer_visible`. Two flags rather than one, because the media
path being up and a human looking at it are different facts and folding them would let either
clobber the other. The health verdict is also suspended while hidden, and **withdraws the strain
flag** — a hidden page that left it set would have the compositor shedding for a viewer that is
not watching, and the viewer would return to a reduced frame rate they never asked for.

**Confirmed in the field `2026-09-13 13:00:12`:**

```
07:30:12.061  browser: page is now hidden
07:30:12.061  viewer's page went off screen — rendering paused; the session is kept  windows=1
```

Same millisecond. Bitrate arriving went **11 432 → 0 kbps** within one sample window, the window
was kept, and no verdict line has been produced since. `framesReceived` froze and `framesDropped`
moved 64 → 112 at the transition — what was already in flight — and then stopped.

**Still unexercised:** the return path. `set_viewer_visible(true)` forces a keyframe so the
picture should resume within a frame rather than waiting up to two seconds for the next periodic
IDR; that has not been observed yet.

---

## I21 — `surface missing from known popups`, logged as ERROR, ~15× a minute

**Observed `2026-09-13 13:27:34` onwards**, repeating in bursts while a Chromium session was
running:

```
✖ ERROR  smithay::wayland::shell::xdg: surface missing from known popups
```

Not ours — the message comes from Smithay's `xdg_shell` handler, at the point where a popup is
destroyed and the surface is no longer in its tracked set. The likely cause is ordinary and
benign: Chromium tears down a popup (a menu, an autofill dropdown, a tooltip) and the destroy
arrives after the surface has already gone, which is a race Smithay logs rather than handles.

**Why it is here rather than ignored:** it is logged at **ERROR**, and an ERROR that fires
fifteen times a minute in normal operation is a monitor that has stopped being able to warn
anyone. `scripts/watch.sh` surfaces it as an anomaly, so every menu click in Chrome now reads as
a fault. Whatever the verdict on the underlying race, the log level is wrong for us.

**Unfixed. Not investigated.** No visible symptom: menus open and close correctly, no client was
disconnected (`ClientData::disconnected` is now instrumented and stayed quiet through the burst),
no crash. Next step is to confirm it is popup teardown rather than a popup we failed to register
— if it is the latter, popup *positioning* is probably also wrong and nobody has noticed.

---

## I22 — a visibility report the socket refused was never retried, so a hidden page was rendered for

**Caught live `2026-09-13 13:56`**, while watching an unrelated reconnect. The server and the
client disagreed completely, and both logs looked healthy:

```
08:26:57  render pacing healthy fps=120.0 target_fps=120 mean_ms=8.3
08:27:17  browser: ANOMALY fps=0.0 kbps=0 framesReceived=371 … vis=hidden unfocused
```

`framesReceived` frozen at 371 for over twenty seconds with `vis=hidden`, while the compositor
rendered, captured and encoded a full 120 fps the entire time — the exact waste I20 was written
to eliminate, reappearing through a different door.

**Cause.** `W.relayVisible` deduped on a flag it set *before* checking whether the send worked:

```js
if (visible === sentVisible) return false;
sentVisible = visible;                       // latched on the attempt…
return relaySend({ type: "viewer_visible", visible });   // …which could return false
```

`relaySendMsg` returns false when the socket is not OPEN. So a `viewer_visible:false` sent during
a link blip — precisely when a page is being backgrounded on a mobile link, which is when the
socket is least likely to be up — marked itself as sent, never arrived, and the dedupe then
blocked every later attempt **for the life of the page**. There is no timer and no re-assert; the
only recovery was a reload.

**Fixed** by latching on the send: `const ok = relaySend(...); if (ok) sentVisible = visible;`.

**The same bug was in `health.js`'s `sentStrain`** and is fixed the same way — a strain report
that arose while the link blipped was silently dropped and never retried.

Both now have regression cases in `scripts/health-check.mjs`. The harness stub had to be
corrected too: it returned `Array.prototype.push`'s length, which is truthy, so it would have
passed either version.

**Third instance this run of the same pattern** — two individually-correct behaviours composing
into a defect that is invisible in either file alone (after I17 and I19). Dedupe-on-change is
correct. Send-may-fail is correct. Together they are a permanent mute.
