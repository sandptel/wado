# research.md — deep dives worth doing later

Questions too big for a run, parked with enough context to start cold. Each states what is
known, what the question actually is, and what would answer it.

Ordered by expected value. Promote to `TODO.md` when one becomes the run.

---

## R1 — Why does 720×1614 @120 drop 2497 frames when 1728×1080 @120 drops none?

**Known.** Server clean in both: 0 stalls, qmax 1, and the *larger* output is the healthy
one, so it is not bandwidth or encoder load. fps also sagged to 46–51.

**Question.** Is the phone's H.264 decoder refusing 1614-tall @120 (level/profile limits,
or a non-standard resolution falling off the hardware path into software decode)?

**How to answer.** `chrome://media-internals` on the phone during the session — look for
`decoder: software` or a decoder-reset loop. Cross-check `decode` ms: if it jumps against
the 2.5–2.8 ms baseline, decoding moved to software. Then test neighbouring heights
(1600, 1608, 1620) to find whether it is the value or the odd dimension.

**Why it matters.** Device-exact resolutions are a headline feature. If arbitrary heights
break hardware decode at high frame rates, the resolution list must know that.

**Update `2026-09-11`, after the VBV fix — did not reproduce.** Same output at 120 fps now
runs 115–122 fps with `framesDropped` flat at 12 (not growing). Previously 46–51 fps and
2497 drops.

**Do not close this.** Two things changed at once, so the cause is not established: the
bitrate ladder raised this config 2000 → 2521 kbps, *and* the VBV moved from frame times to
bits — which at this config took the per-frame cap from ~8 KB to ~21 KB. An 8 KB cap on a
720×1614 IDR is severe enough to be a plausible cause of decoder trouble, which would make
this the same bug as the flashing rather than a decoder limit. Network conditions also
differed.

To settle it: pin the bitrate and A/B the VBV constant alone. If it is the VBV, R1 is closed
as a duplicate and the "odd heights break hardware decode" theory is dropped.

**Update `2026-09-12` — the decode theory is dead, so R1 needs a new cause.** It was briefly
written down that decode cost explains this: `dec` ≈ 9–10.7 ms against an 8.33 ms budget at
120 fps. Withdrawn the same day. `dec` scales with **bits per frame**, not with resolution, so
a *higher* frame rate at a fixed bitrate makes each frame cheaper to decode (~10.0 ms at 60,
~6.9 ms at 90, extrapolating to ~5 ms at 120). See `optimisation.md` O6/O8. The remaining
suspects are the VBV A/B above and bits-per-pixel starvation, not the decoder.

---

## R2 — Confirm or kill the build-starvation theory

**Known.** The only release-build session with stalls (8) and a 234 ms queue spike happened
while the host was compiling on all 20 cores. The client stayed healthy. Correlation only.

**Question.** Does host CPU contention alone reproduce pump stalls on a release build?

**How to answer.** Known-good baseline running, then `cargo build` a large crate mid-session
and watch `worst_queue_ms` and stall count. Compare `nice -n 19` against normal. Read
`/proc/<pid>/status` `voluntary_ctxt_switches` / `nonvoluntary_ctxt_switches` before and
after (`perf` is not installed).

**Why it matters.** If confirmed, never build while the user is testing — and every
measurement taken during a build is void, which affects how past data is read.

---

## R3 — Is the per-packet `.await` in `write_sample` actually costing anything on release?

**Known.** Structure confirmed: `write_sample` awaits once per RTP packet in series, holding
the packetiser mutex across all of them. **The claim that this is a real bottleneck was
withdrawn** — it was measured on a debug build. Release shows 0 stalls at 12 Mbps / 1080p120.

**Question.** Is there a residual per-packet cost that matters at higher bitrates or on a
lossy link, where the NACK responder contends for the same locks?

**How to answer.** Uncensored per-packet timing (see `TODO.md`), histogram rather than a
threshold. Then a deliberately lossy link (`tc netem loss 2%`) to make NACK active, and see
whether the distribution's tail moves.

**Do not** start by capping tokio workers or forking webrtc-rs. Both were considered and
rejected for lack of a measured problem.

---

## R4 — Can host-memory capture be made cheaper?

**Known.** Capture costs 0.2 ms on `vaapi-dmabuf` and 2.1–3.2 ms on `x264-cpu` — ~13×. The
readback, not the encoder, is a large part of why the software tier misses its budget.

**Question.** Is the readback doing anything avoidable — a redundant copy, a format
conversion, a synchronous `glReadPixels` stall that a PBO would hide?

**Why it matters.** Invariant 3 requires the host-memory path keep working forever. It is
the safety net for every driver where DMA-BUF modifiers fail, so its cost is not academic.

---

## R5 — What is the actual glass-to-glass latency?

**Known.** Nothing. Every number is a per-stage figure from unsynchronised clocks, and the
project's stated target (~80–100 ms) is about a number never measured.

**Question.** What is it, over LAN and over cellular?

**How to answer.** The standard trick: point the phone's camera at the host screen showing a
millisecond timer, with the streamed view beside it, and photograph both. The difference is
glass-to-glass and needs no clock sync at all.

**Why it matters.** The headline claim is currently unfalsifiable. It would also settle
whether the remaining per-stage work is worth anything perceptually.

---

## R6 — ✅ ANSWERED 2026-09-12: the 12 Mbps ceiling does **not** survive cellular. It collapses.

**Known.** 12000 kbps at 1080p120 is clean on LAN. webrtc-rs 0.17 has **no congestion
control** in the send path, so nothing discovers a link that cannot take it. `CEILING_KBPS`
(`conf/bitrate.rs`) was the mitigation.

**Answer: collapse, not graceful degradation.** 1080×2422@60 at the ceiling, over the tunnel on a
tethered mobile link:

```
encoded frames dropped — pump full; viewer is seeing stale frames  dropped=60 … 120 … 180 … 240 … 300 … 360
write_sample stall took_ms=829 … bytes=68512 keyframe=true packets=57
write_sample stall took_ms=745 runq_ms=231 bytes=25012
```

**360 frames discarded — six seconds of video — inside about a minute**, and keyframes stalling
too (829 ms on a 68 kB IDR), which is the worst case: the viewer freezes with nothing to recover
from. No downgrade fired, because the tier logic watches *encode* failures and the encoder was
perfectly healthy. Nothing in the system noticed.

**The arithmetic, which is the whole finding.** CBR at 12000 kbps ÷ 60 fps = exactly **25 000
bytes per frame** — which is why nearly every stall sample reads `bytes=25012`: the rate controller
hits its budget on every P-frame. So the encoder asks the link for a steady **1.5 MB/s** while the
write path drains at **60–270 kB/s** when it stalls. **Between 6× and 25× oversubscribed.**

**The ceiling is not a substitute for congestion control.** It is a constant, and the thing it is
protecting against is not. On this link the protective value would have to be under ~2000 kbps;
on a LAN 12000 is correct. No constant is right for both.

### ✅ The confirmation test ran, 07:40–07:45. Lowering the bitrate fixed it.

| | 12000 kbps @ 60 | 5676 kbps @ 120 |
|---|---|---|
| duration | ~9 min | 4 min 45 s |
| `encoded frames dropped` lines | **7** (420 frames) | **0** |
| `write_sample stall` (>100 ms) | **25** | **1** |
| 300-frame windows with `over_budget>0` | **14 of 104** (13 %) | **0 of 5** |
| worst `max_ms` seen | **829** | **3.6** |

**The decisive detail is that `p50` is identical in both: 0.3–0.4 ms.** The write path is not slow —
it is *intermittently blocked*. In the 12000 config, 87 % of windows look perfect (p50 0.3, p90 0.6,
max 1.7) and the remaining 13 % contain spikes of 76, 745, 471, 458, 829 ms. A mean, or p50 alone,
would have called the two configs identical; only the tail separates them.

That is the percentile line earning its keep in the direction opposite to the one it was built for.
`memory/latency/07` records a false "pattern" read off a censored threshold view; this is the same
instrument preventing the mirror-image error — concluding "the pump is fine" from a healthy median
while four seconds of video went missing.

⚠️ **Two variables moved** (bitrate 12000→5676 *and* fps 60→120), so this is not a clean single-
variable A/B. **5676 @ 60** would separate "fewer bits" from "different frame pacing".

### ⛔ …and the very next session at the *same* config dropped frames anyway

07:46:03 started 5676 @ 120 again. By 07:46:50 — **47 seconds in** — it was logging
`encoded frames dropped … dropped=60`. The preceding session held the identical config for
**4 min 45 s with zero drops**.

**So the comparison above is much weaker than it looked, and the "lowering the bitrate fixed it"
reading is withdrawn.** Same config, same daemon, same device, minutes apart: one clean run, one
dropping within a minute. The variable that moved was the link.

**This is the methodological finding, and it outranks the A/B it just destroyed: this link is not
stable enough to support minutes-long A/B comparisons.** Any config test run over a few minutes
here is confounded by link drift, and that includes every test queued in this file. A test needs
either interleaved A/B/A/B within one session, or enough repetitions to average the drift out —
neither of which the current tooling does.

**It also strengthens the case for option 3 below rather than weakening it.** A link whose capacity
swings by enough to take a config from zero drops to sixty in under a minute cannot be served by
*any* fixed bitrate, which is what options 1 and 2's ceiling and what the user's manual adjustment
both are. Adaptive control is not an optimisation here; it is the only thing that can track this.

### What to do about it — a decision, not a task

Three options, and the choice is the user's:

1. **Lower `CEILING_KBPS`.** One constant. Costs LAN users real quality to protect cellular ones,
   and still guesses.
2. **Real congestion control from RTCP.** The correct answer and the expensive one: webrtc-rs 0.17
   gives no bandwidth estimate, so this means wiring TWCC feedback into an estimator and the
   estimator into the encoder. A milestone.
3. **⭐ A local controller off signals already in the log.** `write_sample` stall duration and the
   pump's `dropped` count *are* a congestion signal — one we already compute, on the send side,
   with no dependency on webrtc-rs at all. A controller that lowers the encoder's target when the
   pump backs up and raises it when clear is perhaps 50 lines and uses only what exists.
   **Recommended as the first move**, with the caveats stated honestly: it is a reactive controller
   and can oscillate, it needs a floor and a rate limit on changes, and it is strictly worse than
   real BWE — it responds after the damage rather than before it. It is also testable tonight,
   which options 1 and 2 are not.

⚠️ Whatever is chosen, note that **`runq_ms` and `psi_cpu` contradict each other** throughout this
data (`runq_ms=231` with `psi_cpu=0.0`), so neither can be used as an input until that is settled —
see `reports/2026-09-12-instrumentation.md`.

---

## R7 — Should the client constrain options the active tier cannot deliver?

**Known.** The UI offers 1080p120 with software encode, which is arithmetically impossible
(tick 22.5 ms vs an 8.33 ms budget). Invariant 5 is satisfied — the client does say "encoder
software" — but only *after* an unachievable combination is chosen.

**Question.** Constrain the list, warn beside it, or leave it as a deliberate experiment
surface?

A design decision, not a research one — but it needs R1 and R4 first, since both change where
the real ceilings are.

---

## R8 — "Add vsync": what that can mean here, and which part is missing

**Asked 2026-09-11.** Investigated, not built. Two of the three layers already exist.

**Already done — server pacing.** The render timer is a drift-free fixed-cadence source
(`headless.rs`, the `TimeoutAction::ToInstant` block). Period is `1e9 / fps` ns, phase is
preserved when on time and re-based when behind, and late frames are dropped rather than
accumulated. That is the server-side equivalent of vsync and it needs nothing.

**Already done — client presentation.** Frames land in a `<video>` element that the browser
composites on the display's own refresh. It is vsynced by construction; there is no tearing to
fix and no knob that would help. Nothing to add.

**The part that is actually missing — the two cadences are unsynchronised.** The server
renders at rate `F` with its own phase; the panel refreshes at rate `R` with its own. Even
when `F == R` the phase drifts, so frames land at varying offsets from the scanout and the
result reads as judder that no jitter-buffer setting removes. This is why 120 fps on a 120 Hz
panel can still look uneven.

**Prerequisite before anything can be matched: we do not know `R`.** There is no standard API
for display refresh rate. It is measurable to good accuracy by timing `requestAnimationFrame`
deltas over ~1 s and taking the median. Cheap (~15 lines in the bridge), and it unlocks:
- showing the user what their panel actually does, so the fps choice stops being a guess;
- defaulting fps to the nearest option at or under `R` instead of a hardcoded 60.

**Full phase-locking is a real project, not an add.** `requestVideoFrameCallback` gives
per-frame presentation timestamps client-side; feeding those back so the server nudges its
frame phase is closed-loop control with a stability problem attached. Do not start it before
`R` is known and the bandwidth starvation (`memory/latency/bandwidth.md`) is addressed —
judder at 0.016 bits/pixel is far more likely to be missing bits than missing phase lock.

**Evidence the panel is 120 Hz:** live sessions requesting 120 fps report browser-side
`fps=118–121` sustained, so the display keeps up with 120.

---

## R9 — Measure the panel's refresh rate (promoted out of R8)

**Why it is its own item now.** R8 parked this as a prerequisite for phase-locking, which is a
project. The measurement on its own is ~15 lines and delivers user-visible value without any of
that: the fps picker currently offers 30/60/90/120 with no idea what the panel can do, so the
choice is a guess the user pays for.

**How.** Time `requestAnimationFrame` deltas over ~1 s in the bridge and take the median.
No API exists for this; the rAF cadence *is* the refresh rate.

**What it unlocks, in order of value.**
- Show the measured rate beside the fps picker, so 120 on a 60 Hz panel stops looking like a
  free upgrade.
- Default fps to the nearest option at or under `R` instead of a hardcoded 60.
- Grey out (or warn on) rungs above `R` — they cost bits per frame for frames never shown.

**Cheap, no server change, no protocol.** Do this before anything in R8 proper.

---

## R10 — Does a client actually take the dmabuf path? — **ANSWERED: yes**

**Closed 2026-09-12.** Chrome hands over `AB24` (ARGB8888) under an AMD tiled vendor modifier,
logged by the session itself. What remains is *how much it is worth*, which is `optimisation.md`
O7, not a research question. Left below for the method, and for the two `/proc` measurements
that looked like answers and were not.


**Known.** `zwp_linux_dmabuf_v1` landed in `fa3a1f3`. Before it, `wl_shm` was the only buffer
path, so a GPU application copied every frame through CPU memory twice.

**Question.** Does the Chrome that wado spawns bind it and use `create_immed`, or does it stay
on shm anyway — because it is already on a software GL path, or because its buffer allocation
predates the global?

**How to answer.** `WAYLAND_DEBUG=1` on the spawned client; look for `zwp_linux_dmabuf_v1`
followed by `create_immed`. If only `wl_shm.create_pool` appears, the global is dead weight and
the question becomes why Chrome is not on the GPU at all.

**Why it matters.** The copies it removes run on the render tick, competing directly with the
frame budget. It is one of the few items left that can raise the achievable frame rate rather
than the comfort of the current one. Full measurement plan in `optimisation.md` O7.
