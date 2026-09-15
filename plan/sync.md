# sync.md — making the three rates agree

The problem is `plan/problems.md` §P1. This file is the answer, in three stages, ordered by
what can ship without your attention.

**The constraint you set, and it governs everything here:**

> "we do not need to instantaneously equate these, we need to have a timestep over which the
> fps and refresh rates gracefully sync so a variable interconnection just does not effect
> experience when fps is set to high."

So: no stage below reacts to a single bad second. Every rate change is either (a) forbidden
outright, or (b) gated behind hysteresis that already exists.

---

## §1 — Lock the frame rate. **Shipped 2026-09-13, client-only.**

> "Include a option below fps that locks it like vsync which should not allow this variability"

### What it does

A checkbox under the FPS picker: **Lock frame rate (like vsync)**. When it is on, the client
never tells the daemon it is strained, so `congestion.divisor` never leaves 1 and the output
rate never moves. The rate you picked is the rate you get, for as long as the link can carry it.

### Why the fix is one flag on the client and not a protocol change

The variability has exactly one source: the shed. The shed has exactly one *adaptive* trigger:
the viewer's strain report, sent by `health.js` when its settled verdict names "your device".
Suppress the report and the adaptive path is gone. No new `SessionConfig` field, no new
`RelayMsg` variant, no server arm — and critically **no daemon swap**, which is the thing that
kills your desktop (§P3). Deployed by a client push and a page reload.

The trade this makes, and it is the honest meaning of "vsync": with the lock on, congestion
arrives as **judder and dropped frames** instead of as a clean lower frame rate. That is worse
on paper and better in the hand, because a constant 120 that occasionally stutters feels like
one thing, and a rate walking 120→60→40→60 feels like the connection is fighting you. The UI
hint says exactly this. It is your call per session, which is why it is a checkbox and not a
policy.

### ⚠️ Corrected the same day — what this does **not** fix

Shipped, then traced, and the trace withdrew the justification. **Every shed in the daemon log
— including the one at 13:29:17 that fired while this was being written — carries
`strained=false`.** They are pump-side, and the lock does not touch the pump path (see below).
No viewer strain report reached the daemon at all in the 50 minutes covered.

So the lock does not address what the user reported today. It addresses the viewer-driven walk
measured on 2026-09-12 (16:17 and 16:18, both releasing at divisor 4 — the signature of a loop
feeding on its own output), which is real and which the lock does prevent outright. That is
enough to justify it, and it was asked for by name. It is not enough to call it the fix for
§P1, and this file said it was.

Full working in `plan/problems.md` §P1a.

### What the lock deliberately does *not* disable

The **pump-drop** shed — `congestion` also raises the divisor when the frame pump's outbound
queue backs up, independent of any viewer report. That stays live under the lock, and should:
it is not an adaptive policy about your experience, it is the sender refusing to render frames
it physically cannot hand off. Disabling it would queue frames into a full channel and lose
them a metre further down the same pipe, having burnt the GPU time first.

So the lock is precisely: *ignore the viewer's opinion about the frame rate*. It is not
*render regardless of physics*.

### Files

`js/core.js` (default), `js/settings.js` (`setFpsLock`), `js/health.js` (one guard in
`reportStrain`), `state.rs`, `persist.rs`, `ui/session.rs`, `ui/live.rs` (restore on reload).
`scripts/health-check.mjs` gains the two cases that matter: locked+strained sends nothing, and
locking *while already strained* releases the latch.

---

## §1a — Don't halve the rate for two dropped frames. Designed, not built.

`congestion.rs` halves on **any** drops in a window, by design and stated in its own header. A
window is `WINDOW_TICKS = 60`, which at 120 fps is half a second. The shed today fired on
`dropped_in_window=2` and recovered three windows later:

```
07:59:17.210  from=1 to=2  dropped_in_window=2  strained=false
07:59:18.710  from=2 to=1  dropped_in_window=0  strained=false
```

**1.500 s apart, and `RECOVER_WINDOWS(3) × 60 ticks ÷ 120 fps` = 1.500 s exactly.** The log
confirms the window arithmetic independently.

The cause was a single 33 ms pump stall (`PUMP 1/300 frames over budget max=33.3ms`) — which is
`webrtc-rs` 0.17's blocking `write_rtp` on its 256-deep channel, the wart the pin documents. At
120 fps a 33 ms stall is four frame periods, so it drops a handful of frames, and "any drops"
turns that into a second and a half at half rate. **A transient read as a condition.**

"Any drops" was not wrong when written — it was tuned against a session that discarded **420**
frames, where reacting on the first window was obviously right. Two is not 420, and the
threshold cannot tell them apart because it does not look at magnitude.

Two shapes, and the second is better:

1. a magnitude floor — halve only when the delta exceeds some fraction of the window;
2. **require two consecutive drop windows**, which reuses the file's existing `STRAIN_WINDOWS`
   vocabulary and costs exactly one window (0.5 s at 120 fps) of reaction time. A real
   collapse fails every window; a 33 ms hiccup fails one.

Server-side, so it needs a daemon swap. **Not worth one on its own** — it buys 1.5 s per event
and those events are rare. Batch it with the keyframe-per-flap fix in `plan/TODO.md` the next
time a swap has to happen anyway.

Incidental confirmation worth keeping: `dropped_in_window=2` three seconds after a reattach
proves the drop-counter delta survives `congestion.reattach()` correctly. `reattach()` zeroes
`last_dropped` next to a monotonic sink counter, which looks like it should produce an enormous
first-window delta. It does not.

---

## §1b — What we know about the latency, after a lot of wrong turns

**The finding, and it is the answer to "input does not feel synced with fps":**

On this mobile link at 120 fps, the playout buffer (`jbuf`) sits at **31–54 ms as its standing
state** — before a reconnect, after one, unchanged by either.

```
13:27:47  fps=120  rtt=30  jbuf=44  jtgt=44  dec=4.83   kbps=4031
13:28:07  fps=119  rtt=35  jbuf=54  jtgt=55  dec=5.17   kbps=1974
13:29:44  fps=117  rtt=31  jbuf=47  jtgt=49  dec=42.04  kbps=2774
```

That is **four to six frame periods of delay**, continuously, while `fps` holds at 116–124,
`rtt` sits at 24–45, `lost` is 0 and `framesDropped` never moves. Every metric wado had was
about *rate*; this one is about *delay*, so nothing could see it and the strip said "healthy".

**There is no lever for it.** `js/webrtc.js` already asks for a 20 ms buffer and `jtgt` reads
49–55 against that — the browser's timing model outranks the hint, and the file already carries
the v0.0.2 note proving a re-assert changes nothing. The buffer is the browser's decision.

**What shipped:** a `settling` verdict that names the delay instead of claiming health, scaled in
frame periods (4 frames, 40 ms floor) so a 120 fps session warns at its real 41–54 ms while a
90 fps session's settled 30–37 stays quiet. It reports; it does not fix.

**One coupling worth remembering:** `dec` swung 5 → 42 ms across a reconnect with the frame rate
held. `dec` is what the strain rule keys on, so an excursion like that could ask for a shed on a
phone that is fine. The `keepingUp` gate blocked it — that is the gate saving us, not the rule
being right. Anything that relaxes `keepingUp` must deal with this first.

---

## §1c — Withdrawn. Do not re-derive these

Three explanations for the `dec` 5 → 42 ms excursion were proposed and killed in one session.
Recorded so the next reader does not walk the same three paths.

| Theory | Killed by |
|---|---|
| **The reconnect inflates the jitter buffer** | The before-picture. `jbuf` was 41–54 ms *before* the reconnect and 47–51 after — it never moved. Only `dec` did. |
| **Reconnection causes the excursion at all** | The next reconnect (13:52:38) produced `jbuf` 24–33, `dec` 4.4–5.3 from the first sample, no decay. Reconnection is not sufficient. |
| **Decoder load tracking content bitrate** | `r(kbps, dec) = 0.026` over n=22 within one session. `dec` decays with time-since-connect, not load. *(Caveat: `kbps` spans only 2200–3500 there, so a level difference between a static 1.1 Mbps screen and a moving 3 Mbps one is not excluded — only fine-grained tracking is.)* |
| **A 4-keyframe burst at reconnect** | The 2–4 `forced IDR` lines are requests against an idempotent boolean (`force_idr = true`), not four keyframes. `frames=300 keyframes=2..3` against a 120-frame interval is exactly the periodic rate. Keyframes are 22–23 kB typically, not ~40. |

**`dec` decays with time since connect and nothing else we can see.** That points at something
phone-side — a governor ramping clocks, a decoder pipeline filling — which is not observable from
this end. The measurement that would settle it is the phone's own decoder stats over a session's
first two minutes, and nothing collects that.

**Stop proposing causes for it.** Four withdrawn theories is the signal.

---

## §2 — Let the advertised mode follow a **settled** shed. Designed, not built.

This is the stage that actually answers "sync", as opposed to "stop desyncing".

### The idea

When the divisor has been stable at 2 for long enough to be believed, tell the world. Call
`reconfigure_session` with the same width, same height, same scale, and `fps` divided —
**refresh-only**. Then:

* apps inside the session pace their animation at the rate they will actually get, instead of
  at three times it;
* the client's fps figure — already round-tripped through `SessionReconfigured` — matches the
  frames arriving, so `health.js` stops reading a 40-of-120 shortfall as a fault;
* the input coalescer's assumption becomes true again, because the remote end's rate is once
  more the rate it advertises.

### The two hazards, both already paid for once

1. **A second control loop stacked on the first.** `congestion` adjusts the divisor; a mode
   changer watching the divisor is a loop watching a loop, and loops that watch loops
   oscillate. That is not hypothetical here — it is exactly the 2026-09-12 21:43 strain
   oscillation (`health.js` mixing this tick's decode reading with the settled verdict, divisor
   walking 1→2→1→2 for a minute). **Mitigation: do not add a timer.** Hang the trigger on
   `congestion`'s own `patience` / `RECOVER_WINDOWS` hysteresis, which is the existing timestep
   and is already tuned. A mode change fires only where `congestion` itself would call the
   state settled.
2. **Invariant #8 and I16.** Wayland cannot un-advertise a mode, so this must go through a
   fresh `Output` — which `reconfigure_session` already does correctly. But a fresh `Output` is
   what makes kitty exit (I16). Chrome survives it; kitty does not. **A refresh-only change
   that costs you your terminal is not an improvement**, so §2 is blocked on I16 regardless of
   how good the rest of it is.

### Verdict

Worth building **after** I16. Not before. The ordering is not caution, it is that the benefit
(smoother pacing) is smaller than the cost (an app dying) until I16 is closed.

---

## §3 — Choose a base fps that divides the panel rate. Cheap, partial, needs a measurement.

`js/refresh.js` already measures the panel rate by timing `requestAnimationFrame` gaps and
snapping to a known rung. It is **advisory only** — it renders a hint under the FPS picker and
is never sent anywhere.

If the shed is going to halve the rate anyway, it matters enormously *what it halves to*. On a
120 Hz panel, 120→60 is clean: every frame is shown for exactly two refreshes. On a 90 Hz
panel, 120 was already wrong (frames the panel never shows) and 60 is worse (2:3 cadence —
alternating 2-refresh and 1-refresh frames, which is the classic 3:2-pulldown stutter and is
visible even to people who do not know what they are looking at).

So: **prefer an fps that divides the measured panel rate, and prefer one whose halves also
divide it.** On 120 Hz that is 120 or 60. On 90 Hz that is 90 or 45 — and 45 is not on the
picker.

Shippable piece, small: when `refresh_hz` is known, mark the rungs that divide it evenly and
say so in the hint. Not automatic selection — an automatic pick that silently overrides what
you chose is the same class of surprise §1 exists to remove.

### ✅ The measurement arrived, and it changes the recommendation to a specific number

Step 1's `hz=` / `ratio=` fields produced their first readings after the reload. **65 samples,
all identical:**

```
hz=60 ratio=2.00
```

**The phone's panel is 60 Hz. The session was running at 120 fps.**

So for this entire investigation, *every second frame wado rendered, encoded and transmitted was
discarded by the phone without ever being shown.* Not dropped by congestion, not lost in the
path — displayed nowhere, because the panel has no refresh to show it on.

What that costs, all of it for nothing:

| | at 120 fps on a 60 Hz panel | at 60 fps |
|---|---|---|
| frames the panel can show | half | all |
| bits per displayed frame | **half** of the budget wasted on invisible frames | double — the same link, twice the quality |
| decode work | 120 frames/s decoded, 60 shown | 60 decoded, 60 shown |
| cadence | 2:1 — even, but at half the useful bitrate | 1:1 clean |
| input pacing (`rAF`) | 60 Hz, against a 120 fps stream | 60 Hz, matched |

This also reframes the user's own hypothesis, which opened this whole thread: *"my device just
cannot accommodate 120 fps and it drops to 90 or below variably."* The device was never
*showing* more than 60. What varied was how much of the invisible half got through.

It is very likely part of the `dec` story too — a decoder chewing 120 fps to display 60 is doing
exactly twice the necessary work — though per §1c that is a hypothesis and the count of withdrawn
hypotheses in this file is already four. It is offered as a consequence, not a conclusion.

**Recommendation, concrete: set FPS to 60.** Same link, double the bits per frame, clean 1:1
cadence, half the decode load, and input pacing that matches the stream. There is no trade being
made here — 120 fps on a 60 Hz panel has no upside whatsoever.

### Verdict, measured after the switch to 60 fps

Config, same link, same bitrate budget:

```
before  fps=120  bitrate_kbps=5676  keyframe_interval=120  bits_per_px="0.0181"
after   fps=60   bitrate_kbps=5676  keyframe_interval=60   bits_per_px="0.0362"
```

**`bits_per_px` exactly doubled**, which is the prediction restated as a fact: the same bitrate
across half as many frames, and none of the discarded half was ever visible.

What the phone measured:

| | at 120 fps | at 60 fps |
|---|---|---|
| `ratio` | 2.00 | **1.00** |
| `jbuf` | 41–54 ms | **24–30 ms** |
| `dec` | 5.5 ms baseline, 24–42 in excursion | 6–10 ms |
| delivered fps | 116–121 (half unseen) | 56–61 (all shown) |

**The standing playout latency roughly halved** — 41–54 ms down to 24–30. That is the §1b
finding moving in the right direction for a reason §1b could not have produced: the buffer was
not attacked, the thing filling it was. Half the packet rate is half the arrival jitter for the
browser's timing model to react to.

At 60 fps the scaled `JBUF_WARN` is 67 ms (4 × 16.7), so 24–30 correctly stays quiet.

**This is the answer to the user's original question**, and it is not the one either of us
expected: the fps and refresh rates did not need a control loop to sync them gracefully over a
timestep. They needed the picker to stop offering a rate the screen cannot show.

§1 (the lock) and §1a/§2/§3 remain what they are, but this is the fix that mattered.

**And it makes Step 2 the highest-value remaining item in this file**, not the small nicety it
was written as: the picker already knows `refreshHz` and already renders the warning text. Nobody
read it. Marking the rungs that divide the panel rate — and defaulting to the largest that does —
would have prevented the entire last two hours.

---

## For your attention — the two things you have to decide

1. **§2 is blocked on I16** (kitty exits when the `Output` is replaced). Closing I16 unblocks
   both the refresh-follows-shed work *and* on-the-fly resize for terminal users. It is
   currently the highest-leverage open bug in this branch.
2. **The lock's trade is a preference, not a correct answer.** Try it on LTE at 120 with the box
   ticked. If constant-rate-with-stutter feels better than the rate walk, the default should
   probably change to on; if it feels worse, the honest conclusion is that the shed was right
   all along and §2 is the real fix rather than §1. Either result is worth having.

### §1c addendum 3 — `dec` is bimodal, and it moves with `jtarget`

Observed `2026-09-13 20:12`, two consecutive samples on a degraded link:

```
20:12:32  fps=34  rtt=32  jbuf=8   jtarget=0   dec=15.66  kbps=844
20:12:47  fps=31  rtt=42  jbuf=44  jtarget=44  dec=105.12 kbps=1617
```

**`dec` went 105 → 15.66 → 105 in thirty seconds**, and the sample where it collapsed is the same
sample where `jtarget` read **0** and `jbuf` fell to 8. Both snapped back together.

No cause proposed — four have already been withdrawn in this file and the rule stands. But two
things are now established that were not:

* `dec` is **not** a stable property of the decoder or the content. It is bimodal, and it can
  cross between the two modes within one sample interval.
* It moves **with** the buffer's own numbers, not independently of them. §1c withdrew "`dec` and
  `jbuf` decay in lockstep" on the grounds that `jbuf` had not moved; here all three move at once,
  which is the first evidence that whatever `dec` is measuring shares state with the jitter
  buffer rather than merely coinciding with it.

Worth catching again deliberately: a `jtarget=0` sample is rare and is the one that carries the
information. If `dec` proves to be buffer-hold time plus decode work rather than decode work
alone, every budget comparison keyed on it — including `health.js`'s device rule — is measuring
the wrong thing on a congested link.

**Withdrawn four minutes later.** A fresh peer connection at 20:14:45 — its first samples, 163
frames in — read `jbuf=9 jtarget=11 dec=104.84`. Buffer nearly empty, `dec` already at its high
mode. So `dec` does **not** move with the buffer; the 20:12:32 sample was a coincidence of two
things resetting at once, and I recorded it as a relationship on a single observation. That is
the fifth withdrawn claim about `dec` in this file, and the fourth to come from reading one
sample as a pattern.

What survives is only the first half, which two independent samples now support: **`dec` is
bimodal** — roughly 15 ms or roughly 105 ms, crossing between them within a sample interval, and
capable of starting at 105 on a connection a second old. Nothing here explains why, and this file
is done guessing. The measurement that would settle it is the phone's own decoder stats, which
nothing collects.
