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

**Still to watch:** whether one step (90 → 45 effective) is enough for this phone at
1080x2422, or whether it settles at the `MAX_DIVISOR` floor. The step is deliberately slow, so
give it ~6 s to find its level.

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
