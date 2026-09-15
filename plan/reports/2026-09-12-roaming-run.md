# Run log — 2026-09-12, roaming

The user is away from the laptop, using wado over varying mobile networks, and reporting
through Claude remote control. This file is the **live anomaly log**: what the monitor saw,
which side it was on, and what it costs to fix. Research items graduate to `research.md`,
work items to `plan/TODO.md`.

Rig: relay + daemon restarted 15:36 IST, tunnel deliberately **not** restarted so the quick
tunnel URL survived. Card in `/tmp/wado-rig/CONNECT.md`.

| | |
|---|---|
| Relay | `https://adrian-supporting-broadway-arcade.trycloudflare.com` |
| Remote ID | `872-990-894` |
| Daemon | 14:01 release build, `dd4d4c1` → redeploys tracked below |
| Host link | WAN 0% loss / 5.6 ms, public `59.89.212.117` (routable, **not** CGNAT) |

---

## Baseline — 15:41 IST (10:11 UTC), first session of the run

Home WiFi both ends. This is the number to A/B every later anomaly against.

| | |
|---|---|
| session | 1728×1080 @90, VA-API |
| render | 90.0/90 fps, mean 11.1 ms, max 13.1 ms |
| pump | p50 0.0 / p99 0.2 / max 0.3 ms, **over_budget 0**, queue 0 ms |
| client | fps 91.0, rtt 27 ms, jbuf 17 ms (target 23), dec 9.14 ms, **lost 0** |
| breakdown | capture 0.1 · encode 3.4 · queue 0.0 · tick 11.1 · net ~10.5 · buf 20.0 · decode 8.9 |
| bitrate | ~669 kbps observed, key 17 kB, P 3 kB |

Note `buf=20.0` is the largest single term and it is the **receiver's** playout buffer, not
the network. `net~10.5` on a 27 ms rtt is consistent. Nothing to fix here; it is the reference.

---

## Steps taken this run

| time | step | commit | effect measured |
|---|---|---|---|
| 15:35 | monitor summariser — daemon log → one tagged line per event, sides named | *(local, `/tmp/wado-rig/watch.sh`)* | ANSI stripping was required; every `key=value` read `?` without it |
| 15:44 | ⌨ button → `<label for>` so the phone keyboard opens | `fc386bc` | deployed, wasm `dxhf9b7bd4c6e113f5` (moved from `dxhceeb40ab2c93eaf0`, `wado-osk` ×4). **Unverified by a human** |
| 15:50 | health verdict: names the side, shows needed vs available bandwidth | `fc7df2d` | 6/6 rule cases pass (`node scripts/health-check.mjs`). Deployed: wasm `dxh3deb17a1c9a1fe6e`, `"link offers"`×2, `health-` css, `setTargetKbps`×2. Daemon swapped 15:50 so the bandwidth half has its yardstick |
| 15:48 | `systemd-inhibit` holds `sleep:idle:handle-lid-switch` so the laptop can be locked without killing the rig | *(local, pid in `systemd-inhibit --list`)* | verified present in the inhibitor list |
| 15:51 | monitor attributes each client anomaly against the server's state in the same second | *(local, `watch.sh`)* | 3/3 synthetic cases attribute correctly (network / device / server) |

### Untested candidate cause, parked

`802-11-wireless.powersave = 0 (default)` is "use the global default", not "off". If the
connection dies **specifically after the laptop is locked**, this is the first suspect, ahead
of anything in wado.

---

## ⚑ The finding of the run — 16:35

The user went the opposite way to the recommendation: **up** in resolution and bitrate, **down**
in frame rate. It is decisively better, and it refutes what was predicted here beforehand.

| | `720×1614 @90`, 2521 kbps | `1080×2422 @60`, 11353 kbps |
|---|---|---|
| bits/frame | 28 kbit | **190 kbit** (~7×) |
| decode budget | 11.1 ms | 16.7 ms |
| measured `dec` | 8.9 ms → **collapsing to 23–113 ms** | **10.92 ms**, steady |
| headroom used | 80% → far over | **65%** |
| client fps | 8–44 under load | **59–60 of 60** |
| dropped | 60–90 per second | **1 in 1475 frames** |
| server | clean throughout | clean (render 60.0/60, pump p99 1.0 ms, over 0) |

**Prediction made before the data and refuted by it:** that seven times the bits per frame
would cost more decode and make this worse. Decode did rise — 8.9 → 10.92 ms — but the budget
rose further, and the total decode work per second *fell* because there are a third fewer
frames. **On this phone, fps is the decode constraint; resolution and bitrate are not.**

This is the same shape `memory/latency/measurements.md` already recorded from the other end —
decode flattens at a ~7 ms per-frame floor, so per-frame overhead dominates and halving the
frame rate nearly halves the load. It had not been drawn as a *recommendation* before.

**Practical rule: on a phone that is dropping frames, cut fps before touching anything else.**
Higher resolution at 60 is better than lower resolution at 90 — which is the reverse of the
intuition that a smaller picture is easier.

## Feature landed — `zwp_text_input_v3`, the keyboard that raises itself

User confirmed the ⌨ button works ("needs forced open/close") and asked for the automatic half.
Both halves live 16:50: daemon `0c76378` (16:49 build), client wasm `dxhb98955188b4a3cd5`
(`W.textInput` ×2).

**The decision worth keeping.** Smithay's `TextInputManagerState` discards every request unless a
client has bound `zwp_input_method_v2` (`text_input_handle.rs:209`, `has_instance()`), and an
instance can *only* be created through that manager's dispatch — no server-side registration
exists. wado would have had to become a second Wayland client of its own display
(`wayland-client`, a socketpair, a second event loop) to obtain one boolean. So wado implements
`zwp_text_input_v3` itself as a pure observer in `handlers/text_input.rs`, and **must not also
register smithay's** — two globals of one interface would both be advertised.

Deliberately an empty IME: `enable`/`disable`/`commit` accepted, `enter`/`leave`/`done` sent,
`preedit_string`/`commit_string` never. Identical to Sway and Hyprland with no IME running, where
typing works because toolkits still read `wl_keyboard` — which is where wado's synthesized keys
arrive. **Do not extend into a real IME without deciding what would drive it.**

Two protocol traps, both handled: `enable` is double-buffered so the signal fires on **commit**;
`leave` must go out on focus loss or the client keeps a surface it no longer owns.

Close is asymmetric on purpose — auto-close only ever undoes an auto-open, so a toolkit emitting
`disable` during focus churn cannot shut a keyboard the viewer opened deliberately.
`scripts/osk-check.mjs` pins both directions, 5/5.

**If it misbehaves, the discriminator is in the monitor:** `⌨ TEXTIN` present with no keyboard ⇒
the client half; no line at all ⇒ the app never bound the protocol, and some toolkits need
`GTK_IM_MODULE=wayland` / `QT_IM_MODULE=wayland` in the session environment.

## Anomalies seen live

### A5 · 16:31 — two constraints at once, and a recommendation that follows from both

Same config every time: `720×1614 @90`, 2521 kbps.

| | reading | side |
|---|---|---|
| link estimate | 278 kbps → 1.4 Mbps, against a 2.5 Mbps target | the link |
| decode | 23–76 ms against an 11.1 ms budget, `lost=0`, server clean | the phone |

**Both are real and independent.** No encoder setting closes a 2.5 Mbps ask on a 280 kbps
link, and no link improvement fixes a decoder taking seven times its budget.

**The pattern across A2 and A5 is the same and it is now three sessions deep: this phone does
not hold 90 fps at this resolution.** A2 managed 8.9 ms for eighty seconds before collapsing;
A5 was over budget almost immediately, on a session that started cold. That weakens the pure
thermal reading — cold start should have bought another eighty seconds and did not.

**Recommendation to the user: 30 or 60 fps.** It is the one change that addresses both
constraints at once — a third of the decode work per second, and a third of the bit rate.

**Still the same missing feature.** wado has no adaptive response to either constraint: it
sends 90 fps at 2.5 Mbps into a phone doing 39 fps on a 1.4 Mbps link, indefinitely. See the
fps-backoff item in `plan/TODO.md`.

### A6 · 16:00:21 — WebRTC up, no session behind it · **a state bug, ours**

After a session stopped, the viewer reconnected: ICE reached `connected`, `viewer connected
via WebRTC` was logged, the data channel carried pings — and every input was discarded with
`input dropped — no active session`. The user tapped (`Touch { id: 28, phase: Down }`) and it
went nowhere. From the sofa: a frozen picture and dead taps, which reads as a hang.

`watch.sh` now flags this as `✖ ORPHAN`. The underlying question — should a reconnect with no
session restart one, or tell the viewer plainly — is **unresolved and belongs with the rejoin
work** (`f599da4`). Filed.

### A3 · 16:27:41 — a real congestion dip, and an attribution bug it exposed · **the link**

Session `720×1614 @90`. Within three seconds:

```
✖ STALL  201 ms write_sample  runq=0 ms
⚠ SHED   render ticks dropped: 1 in 2 (was 1 in 1), 12 dropped in the window
⚠ CLIENT fps=17 rtt=688 ms lost +7, link estimate 2.7 Mbps → 384 kbps
```

**Side: the link.** `runq=0` is the discriminator and it is decisive — the pump thread was not
waiting for CPU, it was blocked on the socket. The compositor shedding ticks was the *correct
response* to that, not a second fault. The stack handled the dip and recovered.

**Bug this exposed in the monitor, now fixed.** It printed
`↳ SERVER the server was struggling ... (render 90.0/90 fps, pump over=0)` — an accusation
quoting the accused's own clean numbers. It had set its server flag on any stall regardless of
`runq`. A stall with `runq ≈ took` is CPU starvation and is ours; a stall with `runq ≈ 0` is
congestion and is not. Blaming the server here would have cost an hour.

### A4 · 16:26–16:29 — the verdict cried wolf. Three design faults, all mine

The strip was confirmed working end to end (verdict lines reached the daemon log from the
phone — the first of the run's unverified features closed without the user looking). It was
also wrong three ways, and every one was found by *watching it run*, not by reading it:

1. **Dedupe keyed on `detail`**, which carries a decode time that moves every tick — so "relay
   on change only" meant "relay every second". It flooded the monitor until the harness killed it.
2. **`availableIncomingBitrate` is not spare capacity.** Chrome tracks the *received* rate with
   it while nothing is congested, so a static screen sending 600 kbps reports a "600 kbps link".
   The rule read that as a broken network, continuously, for ~50 s while nothing was wrong. The
   ramp from 500 kbps to 2.6 Mbps over the first minute is BWE converging, not a link improving.
   **Never treat that field as capacity.** It is evidence only alongside harm (fps short, or loss).
3. **The first seconds of a connection are ramp, not steady state**, and every rule reads a
   one-second rate. Judging that window said "bad your device" two seconds into every session.

Fixed in `842b630`. The rule set now has 7 cases including the two that produced (2) and (3).

### A2 · 16:00 — the phone's decoder collapsed mid-session · **receiver side, sustained**

Session `720×1614 @90`, VA-API, 2521 kbps, bpp 0.0241. Started 15:58:49.

| window | client `dec` | client fps | framesDropped/s | server |
|---|---|---|---|---|
| 15:58:49 – 16:00:07 (~80 s) | **8.9 ms** | 90 | ~0 | render 90.0/90, pump p99 0.4 ms, over 0 |
| 16:00:08 – 16:01:20 (72 s+) | **34 – 113 ms** | 8 – 44 | 55 – 90 | *identical* — render 90.0/90, pump p99 0.4 ms, over 0 |

`framesReceived` kept climbing at ~90/s throughout, and `lost` stayed at 0/-1. **The frames
arrived and were thrown away after arrival.** The server never varied by a single metric.

**Side: the phone, unambiguously.** Decode took 3–10× its 11.1 ms budget.

**⚠️ Withdrawn on the spot:** the first hypothesis pushed to the user was that the **1614 px
height** (1614 mod 16 = 14, so the decoder crops an unusual 1616→1614) forced a software
fallback. Own data refutes it — the *same* resolution decoded at 8.9 ms for the first eighty
seconds. A property of the stream geometry cannot switch on at t+80 s. Do not retry this
explanation for this anomaly; it remains a live candidate for R1 (`720×1614 @120` dropping
2497 frames from the start), which is a different shape.

**What fits the onset:** something on the phone changed after ~80 s of sustained 90 fps
decode. Thermal or power throttling is the leading candidate; another app taking the decoder
or the CPU is the other. Both are off-device and neither is visible in any wado log.

**Measurement that separates them.** Thermal is a *duration* effect and repeats:
1. reconnect on a cool phone at the same settings — if `dec` is ~9 ms again and collapses
   after a comparable interval, it is thermal;
2. reconnect at **60 fps** — one third less decode work per second. If `dec` stays at ~9 ms
   indefinitely, it is load-dependent, which thermal is and a rival app is not;
3. if it collapses immediately on a cool phone, the cause is not duration and the theory dies.

**The gap this exposes, which *is* ours.** The server sent 90 fps into a decoder managing 15
for 72 seconds and never noticed. Nothing in wado reacts to receiver-side collapse: the
client measures `dec` and `framesDropped` and only *displays* them. The fix is a
**client-driven fps backoff** — the client already computes the verdict (`js/health.js`), and
the relay data channel already carries client→server messages, so the missing piece is a
"reduce to N fps" request and a server-side handler that reconfigures the encoder. Sized
against the architecture that is: one protocol message, one match arm in `relay_client`, and
the compositor already rebuilds an encoder on a settings change. **Filed, not started** — two
features are already stacked awaiting human verification.

### A1 · 15:41 — decode time spiked 8.9 → 17.2 ms at session end · **receiver side**

Last three client samples before the user stopped: `dec` 9.14 → 17.19 → 12.46 ms against an
11.1 ms budget at 90 fps. Everything else was clean — 0 lost, rtt 19–33 ms, pump p99 0.3 ms,
render 90.0/90.

**Side: the phone.** Nothing on the server or the path moved. Two readings fit and they have
different fixes:

1. the phone thermally throttled or lost its foreground CPU share as the page was being
   closed — in which case it is an artefact of teardown and means nothing;
2. decode genuinely has no headroom at 90 fps on this device, and the 8.9 ms steady state is
   only 80% of budget, so a modest disturbance pushes it over.

Reading 2 is the one worth knowing, and `measurements.md` already records the shape: decode
flattens at a ~7 ms floor, so 90 and 120 fps both spend most of their budget. **Measurement
that separates them:** watch `dec` across a session that is *not* ending — if it sits at 8.9
and only ever spikes at teardown, it is (1). The health verdict now surfaces this without
anybody reading a log, which is the cheap half of the answer.

No fix proposed yet. Not worth acting on a two-sample spike at teardown.

