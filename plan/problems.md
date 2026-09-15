# problems.md — the experience problems, stated as mechanisms

One file, one job: what is actually wrong, with the evidence. Proposed fixes live in
`plan/sync.md`. Reported-but-unfixed defects live in `issues.md`. This file is for the
*problems behind* those — the ones that are a consequence of the design rather than a bug in it.

---

## P1 — three rates, none of them matching, and only one of them moves

**Reported 2026-09-13 by the user, on LTE, at fps=120:**

> "I am on lte so my device just cannot accommodate 120 fps and it drops to 90 or below
> variably and the device refresh rate must remain the same on the input does not, so input
> does not feel synced with fps."

The intuition is right and the mechanism is worse than the report. There are **three**
independent rates in a wado session. Two are constant. One moves, and it is the only one the
user actually sees.

| rate | what sets it | value at fps=120 | moves? |
|---|---|---|---|
| **Output rate** — frames that leave the encoder | `ec.fps / congestion.divisor` | 120, 60, 40… | **yes** — the shed moves it every few seconds |
| **Input sample rate** — how often the browser sends pointer state | one send per `requestAnimationFrame` = the phone's panel Hz | 120 (or whatever the panel is) | no |
| **App animation timing** — what apps inside the session believe | the advertised `wl_output` mode refresh | 120, always | no |

Evidence for each, in the source:

* **Output rate.** `crates/compositor/src/headless.rs` gates the render tick on
  `state.congestion.should_render(dropped_total, state.viewer_strained)`, and
  `crates/compositor/src/congestion.rs` raises `divisor` to 2, then 3, under strain. A shed is
  not a dropped frame — it is a *rate change*, and it is meant to be one.
* **Input sample rate.** `crates/client/src/js/input_coalesce.js`:

  ```js
  queue(kind, payload) {
    this._pending.set(kind, payload);
    if (this._raf == null) this._raf = requestAnimationFrame(this._flush);
  }
  ```

  with the comment — and this is the assumption that is now false —

  > *"Coalescing caps the rate at the display's refresh, which is the fastest rate the remote
  > end can actually show anyway."*

  That was true when the remote end rendered at a fixed rate equal to `fps`. It stopped being
  true the day shedding landed (`47ea650`). The remote end now shows at `fps/divisor`, and the
  coalescer is pacing against a number that no longer describes it.
* **App animation timing.** `build_output` in `headless.rs` sets
  `Mode { refresh: (ec.fps * 1000) as i32 }`. **The shed never updates this.** A compositor
  emitting 40 frames a second is still telling every application on it that the display runs at
  120 Hz. A client that paces its own animation off the advertised mode — which is exactly what
  `wl_surface.frame` callbacks plus the mode are for — is animating three times faster than the
  frames it will get.

### Precisely what the user is feeling

Not "input is sampled at the wrong rate". The coalescer never invents or drops motion: a finger
that travels 200 px produces 200 px of displacement whether it is sent in 12 packets or 24. What
changes is the **rate at which the response to that motion is displayed**, and that rate is
moving. Uneven display of even motion is judder, and judder that tracks your finger reads as
input lag even though no input was delayed. That is why it "does not feel synced" rather than
"feels slow".

### Why fps=120 is the worst case for it

Higher `fps` means a smaller frame budget, which means the link fails the budget sooner, which
means the divisor moves more often. At 120 a shed to divisor 2 is a drop of **60 frames a
second**; at 60 the same shed costs 30. The setting most likely to trigger the rate change is
also the one where the rate change is largest.

### Status

Half-addressed. See `plan/sync.md` §1 — the frame-rate lock ships now. §2 and §3 are design,
not code.

---

## P2 — `vis=` is sampled at log time, not over the sample window

Found while chasing P1. `stats.js` appends `vis=<document.visibilityState>` to the rlog line,
read at the moment the line is written. The *numbers* on that line describe the second before
it. A page that was hidden for most of that second and became visible just before the tick logs
`vis=visible` beside numbers produced while paused — which is how `fps=0 vis=visible` appears
and looks like a stall.

Not worth fixing with a second field; worth knowing when reading a log. If it ever does need
fixing, the honest form is "was this window *entirely* visible", latched across the interval
rather than sampled at the end.

---

## P3 — the daemon restart still kills the desktop

Unchanged and unsolved, recorded here because it shapes what can ship. Every server-side change
requires a daemon swap, and a daemon swap ends the session and every application in it. That is
why `plan/sync.md` §1 is deliberately client-only: a fix the user can have without losing their
windows beats a better fix they have to schedule.

The real answer is the supervisor milestone in `WADO_PLAN.md` — compositor in a child process,
transport in the parent — at which point the transport can be replaced under a live desktop.

---

## P1a — **correction, same day.** The shed that was felt was the *pump*, not the viewer

Written after §P1 shipped its fix, and it withdraws part of the reasoning behind it.

The lock in `plan/sync.md` §1 suppresses the **viewer-strain** path into `congestion`. Having
shipped it, I went to trace it in the field and found that path had not fired at all.

**Every shed in the current daemon log — the whole 50-minute window, covering the exact minute
the user reported — carries `strained=false`:**

```
07:59:17  shedding … the pump could not take the frames we were making  from=1 to=2  dropped_in_window=2 strained=false patience=3
07:59:18  shedding … clean windows — easing back toward full rate       from=2 to=1  dropped_in_window=0 strained=false patience=3
```

That is 13:29:17 IST — the SHED the monitor raised while §P1 was being written. `strained=false`
on both lines: no viewer strain report reached the daemon in this log at all.

**So the lock, as shipped, would not have prevented the event that prompted it.** It is still
correct and still worth having — the viewer-driven walk is real and was measured on 2026-09-12
(16:17 and 16:18, both releasing at divisor 4, the signature of a loop feeding on its own
output) — but it is not the fix for what was felt today, and saying otherwise would have been a
fix justified by reasoning and reported as verified.

### What the numbers actually say

Delivered frame rate, last 12 samples at fps=120:

```
13:30:09  fps=120.0  kbps=3182  dec=33.67  lost=0
...
13:31:04  fps=119.0  kbps=2984  dec=27.69  lost=0
```

116–121 fps, no loss. The rate is *not* walking now. Over the wider window (400 samples, which
spans an earlier 90 fps session, so read it as a spread and not a distribution):
`p10=59 p50=90 p90=120`.

Two things stand out and neither is the viewer's decoder:

* **Arriving bitrate is 2.5–3.0 Mbps against a 5.676 Mbps target, with `lost=0`.** Nothing is
  being discarded in the path; the encoder is simply not filling its budget, which is what a
  mostly-static screen looks like. The link is not the binding constraint at this moment.
* **Decode is 24–34 ms against an 8.3 ms budget at 120 fps** (p90 41, p99 124) — and the frame
  rate is met anyway. This is the pipelining result from earlier in the run restated: decode
  time is *latency*, not capacity. A decoder can hold per-frame latency four times the frame
  interval and still deliver every frame.

### The real mechanism — and it is a threshold, not a policy

From `congestion.rs`, stated in its own header:

> *"drops halve the rate on the first window that sees any, because by the time drops are plural
> the viewer has already seen it"*

A window is `WINDOW_TICKS = 60`, which **at 120 fps is half a second**. The shed above fired on
`dropped_in_window=2`. Two frames. Against the pump report from the same minute:

```
13:29:19  PUMP  1/300 frames over budget  p99=0.5ms max=33.3ms (budget 8ms)
```

One frame in three hundred over budget, a single 33 ms stall — which is `webrtc-rs` 0.17's
blocking `write_rtp` on its 256-deep bounded channel, the known wart the pin in `CLAUDE.md`
describes. At 120 fps a 33 ms stall is four frame periods, so it drops a handful of frames, and
"any drops halve the rate" turns that into **1.5 seconds at half frame rate** (three clean
recovery windows at 120 fps).

*One 33 ms hiccup costs a second and a half of halved frame rate.* That is the rate walk, and
it is a transient being read as a condition.

The "any drops" threshold was not wrong when it was written — it was tuned against a session
that discarded **420** frames, where reacting to the first window was obviously right. Two is
not 420. The threshold has no way to tell them apart because it does not look at magnitude at
all.

**Proposed fix in `plan/sync.md` §1a.** Requires a daemon swap, which ends the live desktop.

---

## P1b — the session was running at twice the panel's refresh rate the whole time

`hz=60 ratio=2.00`, 65 samples, unanimous. The phone's panel is **60 Hz**; the session was at
**120 fps**.

Every second frame was rendered, encoded, transmitted and decoded to be displayed nowhere. This
was true throughout every measurement in §P1 and §P1a, and it was visible in the UI the whole
time — `js/refresh.js` measures the panel rate correctly and the FPS picker renders *"Above that,
extra frames are never shown — and every frame gets fewer bits"* when the chosen rate exceeds it.

**The hint was right, it was displayed, and it was not acted on.** That is the fifth measurement
this run that existed and went unconsulted (after the empty `disconnected` handler, `SentKbps`,
`visibilityState` and `refreshHz` itself). A warning nobody reads is not a working control, and
the fix in `plan/sync.md` §3 Step 2 — default the picker to a rate that divides the panel — is
the difference between advice and a decision.

Fix for the user, now: **set FPS to 60.**

---

## P4 — the adaptive shed cannot fire on a link that reconnects faster than the verdict settles

**Observed `2026-09-13 20:14–20:15`.** The health strip reached the correct device verdict:

```
20:14:56  bad your device — decode 104.7 ms against a 11.1 ms budget  (try 60 fps)
                            [got 5.3 Mbps of 5.7 Mbps]
```

The network had recovered (5.3 of 5.7 Mbps arriving), so the path was no longer the constraint
and the device rule correctly took over — the discriminator chain worked exactly as designed.

**And nothing happened.** No `viewer reported a change in decoder strain` on the daemon, no shed.
The viewer sat at ~30 fps with a 105 ms decode and the compositor kept sending 90.

**Why.** Three separate mechanisms each restart on a new peer connection, and this link was
re-offering every 20–30 seconds:

* `health.js` needs `SETTLE_TICKS` of a *consistent* verdict before it becomes the shown one, and
  `reportStrain` is keyed on the **settled** side (deliberately — see the 2026-09-12 21:43
  oscillation).
* `startStats` is per-peer-connection: a new pc resets every `last*` and the verdict's inputs
  with it.
* `set_viewer_attached` clears `viewer_strained` on the compositor on every attach, by design —
  a rejoining viewer must not inherit the last one's shed.

Each of those is right on its own. Together they mean **a viewer that reconnects faster than the
verdict settles can never ask for a shed**, no matter how badly it is struggling. The adaptive
path is silently unavailable in precisely the conditions that most need it.

**Not a regression and not obviously fixable by loosening any one of the three** — each guards a
failure that was measured. The honest options are a strain signal that survives a reconnect
(carried in the crumb, or latched server-side against the session rather than the viewer), or
accepting that a churning link is a manual-settings problem and making the strip say so.

**Unfixed.** Recorded because the mechanism looks healthy from every individual angle and is
inert in aggregate — which is the third instance this run of components that are each correct
composing into something that does not work (I17, I19, I22, I24, and now this).
