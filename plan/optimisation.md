# plan/optimisation.md — squeezing the daemon

A research entity, not a task list: things worth *exploring* to make wado hold its frame
budget under load. Each entry says what is known, what the question is, and what would
answer it. Promote to `TODO.md` when one becomes the run.

⚠️ **Nothing here is a fix until a number says so.** Two knobs in this file are already
shipped and neither is yet proven to change anything measurable. Read
[`memory/shared/verification.md`](memory/shared/verification.md) before claiming one works.

Last updated: `2026-09-12`

---

## O1 — Apps escape the CPU weight, and the escape is partial ⚑ ON HOLD

> ⚠️ **Demoted from HIGHEST VALUE 2026-09-11.** The first `runq_ms` reading (O2) shows a
> 106 ms stall with **zero** runqueue delay. CPU share cannot help a thread that was never
> waiting for CPU. Confirm O2 with more samples before spending anything here.

**Known, measured live 2026-09-11 18:06.** Session apps are spawned into a
`systemd-run --user --scope` at `CPUWeight=50` (`compositor/src/proc.rs`). It works — and
Chrome partly escapes it anyway:

| cgroup | weight | contents |
|---|---|---|
| `wado-app-907645001.scope` | **50** | 16 Chrome processes (helpers, renderers) |
| `app-com.google.Chrome-910248.scope` | **100** | Chrome's main process, which re-parented itself |
| `tmux-spawn-….scope` (wado itself) | **100** | the daemon |

Chrome asks the user manager for its own scope at startup, so the process most likely to
cause a demand spike is the one least constrained. This is the same class of escape as the
`setsid` hole in the process-group cleanup, via a different mechanism.

**The question.** Lowering applications is escapable by design — any app may ask systemd for
its own scope. Is the right asymmetry to *raise wado instead*?

**What answers it.** `/sys/fs/cgroup/<wado's own scope>/cpu.weight` was verified **writable**
by the daemon's own uid. So wado can raise itself at startup (weight 100 → e.g. 500) and no
application can escape that, because it is not applied to the application at all. One write,
no dependency on what the app does.

**Before building it:** confirm `runq_ms` (O2) is non-zero. If the stalls are not CPU
starvation, neither weight matters and this is a feature with no measured effect.

**Open trade-off, do not silently pick:** raising wado's weight affects everything else on
the user's desktop, not just the session. A user's own browser outside wado would lose to
it. Lowering apps is polite and escapable; raising wado is effective and rude.

---

## O2 — Is the 300–500 ms stall CPU starvation at all?

**Known.** Stalls are a roughly constant blocking event per frame, uncorrelated with frame
size or packet count (r = -0.03 over 23 stalls ≥200 ms). Bitrate and encoder preset are ruled
out: dropping 12000 → 8100 kbps and Veryfast → Ultrafast did not remove them. They cluster in
time, which is the signature of a bursty external cause.

**The question.** Was the pump thread runnable-but-unscheduled, or blocked on something else?

**What answers it.** Already deployed: `runq_ms` on the stall log, from
`/proc/thread-self/schedstat`, plus `psi_cpu` / `psi_mem`. Verified that run delay still
accumulates on this host despite `kernel.sched_schedstats=0`.
- `runq_ms ≈ took_ms` → starvation. O1 is the fix.
- `runq_ms ≈ 0` → not CPU. Follow `psi_mem`: reclaim driven by a browser allocating stalls an
  unrelated thread for hundreds of ms while every CPU metric says the machine is idle.

**FIRST READING, 2026-09-11 18:35 (n=1):**

```
write_sample stall took_ms=106 runq_ms=0 psi_cpu=0.65 psi_mem=0.0 bytes=37610 packets=31
```

**`runq_ms=0` against `took_ms=106`.** The pump thread spent zero nanoseconds
runnable-but-unscheduled. It was not waiting for a CPU. `psi_cpu` 0.65% and `psi_mem` 0.0 say
the machine was not under pressure of either kind.

**This is one sample and needs more before it is load-bearing** — but if it holds, it
**removes the premise under O1.** Giving wado a larger CPU share cannot speed up a thread that
was never waiting for CPU. Do not do the O1 work on the strength of this entry until n is
larger.

**Where it points instead:** the block is *inside* `write_sample`. `CLAUDE.md` already records
that webrtc-rs 0.17's `TrackLocalStaticRTP::write_rtp` does a **blocking send on a 256-deep
bounded channel, once per RTP packet**. A full channel awaiting drain is exactly a long
`took_ms` with `runq_ms=0`. That is the next hypothesis to test, not CPU weight.

⚠️ Note this is the *sender* side and is a different mechanism from the receiver-side jitter
inflation in O6 — do not merge the two without evidence linking them.

---

## O3 — WITHDRAWN: the VBV cap does *not* explain Balanced

**This entry previously claimed** that the absolute 64 KB VBV cap made Balanced burstier than
Quality by giving it a longer buffer window in time. **That was wrong and is withdrawn.**

**How it was wrong.** It rested on `key_max_kb` compared across *different live browsing
sessions* — different screen content. A 142 KB IDR during a page load and a 99 KB IDR on a
static page say nothing about the encoder. Classic content confound.

**The controlled measurement** (identical `testsrc2` input, 1728×1080@60, 8 s, only bitrate
differing) shows the encoder is **perfectly monotonic**:

| preset | kbps | IDR avg | IDR max | P avg | IDR ÷ VBV |
|---|---|---|---|---|---|
| Reactivity | 4050 | 27.9 KB | 36.8 KB | 9.0 KB | 1.11× |
| Balanced | 8100 | 43.0 KB | 50.8 KB | 18.8 KB | 0.79× |
| Quality | 12000 | 52.7 KB | 58.1 KB | 28.3 KB | 0.91× |

More bitrate → bigger frames, as it should be. IDRs land at or under the VBV, so radeonsi
**does** respect `rc_buffer_size`. There is no encoder-level anomaly at Balanced, and the
presets do not need relabelling on these grounds.

**Reproduce:** `ffmpeg -f lavfi -i testsrc2=s=1728x1080:r=60 -t 8 -vaapi_device
/dev/dri/renderD128 -vf format=nv12,hwupload -c:v h264_vaapi -rc_mode CBR -b:v <R> -maxrate
<R> -minrate <R> -bufsize <B> -g 120 -bf 0 -f h264 out.h264`, then `ffprobe -show_entries
frame=pkt_size,pict_type`. **Never compare presets across live sessions again.**

---

## O3b — MOSTLY RESOLVED: the plateau was idle content, not a ceiling

**Known.** Delivered throughput, measured per preset at 1728×1080 on the live rig:

| preset | target kbps | delivered (median) | deficit | jbuf |
|---|---|---|---|---|
| Reactivity | 4050 | 2825 | 30% | **14 ms** |
| Balanced | 8100 | 3286 | 59% | 20 ms |
| Quality | 12000 | 4088 | 66% | 20 ms |

Delivered bits plateau around 3–4 Mbps whatever is asked for, while the jitter buffer rises
6 ms from Reactivity to the other two. Browser-side latency is **identical** for Balanced and
Quality at 120 fps (jbuf 20.0 ms, n=266/297); Reactivity is the outlier at 14.0 ms. Stalls per
1000 frames actually favour Balanced over Quality (0.6 vs 0.9).

**So the user's "Balanced is worse than both" is not reproduced by any server- or
browser-side number.** Balanced ≈ Quality on every latency metric; both are mildly worse than
Reactivity. Do not relabel on the strength of the earlier (confounded) table.

**The question.** Is the plateau a real link ceiling, or is CBR simply not being filled
because an idle desktop has nothing to encode? `rtt=0` throughout argues against congestion.

**ANSWERED 2026-09-11, from data already collected — no new run needed.** The controlled
`testsrc2` A/B in O3 recorded mean frame sizes at 1728×1080@60, g=120, 8 s. Turning those back
into throughput (`4×IDR + 476×P` over 8 s) gives what the **encoder** produced against each
target:

| target kbps | encoder produced | % of target |
|---|---|---|
| 4050 | 4.50 Mbps | **111%** |
| 8100 | 9.34 Mbps | **115%** |
| 12000 | 14.01 Mbps | **117%** |

**The encoder slightly overshoots every target when the content demands it.** It is not the
ceiling, and neither is VA-API or the preset. The 2825/3286/4088 figures were measured on a
**live idle desktop**, where CBR has nothing to spend bits on — the same content confound as
O3, one layer further down the pipe.

**So the labels are honest.** "The options don't denote reality" is **not supported**: ask for
12 Mbps with something moving and the encoder makes 14. Do not relabel the presets.

**What is still open** is narrower, and must not be overstated: **live sessions have never
been measured with a comparable server-side byte count.** The 14 Mbps figure is `testsrc2` at
1728×1080@60; the ~8.2 Mbps live figure is the phone at 1080×2422@120 running real Chrome —
different resolution, fps, keyframe interval *and* content. Subtracting them and calling the
remainder a pipeline gap would be the O3/O3b confound a third time. The live content may
simply never have demanded more than 8.2 Mbps.

**What answers it:** sum encoder output bytes per second server-side and log it next to the
browser's `kbps`, same session, same content, both ends. The per-frame byte count already
exists — `bytes=` on the stall line — it is just only logged on stalls.
- server ≈ browser → content-limited, O3b fully closed, and O2 stays a separate problem.
- server ≫ browser → a real encoder-to-wire gap, and *only then* is a link to the O2 stall
  (`took_ms=106`, `runq_ms=0`, blocked inside `write_sample`) earned.

⚠️ Caveat: these are recomputed from recorded *mean* frame sizes, not a fresh byte count. The
overshoot is consistent across all three arms and far outside rounding, but a direct
`bytesSent` measurement would be stronger.

---

## O4 — Build profile: what is left after LTO

**Done.** `lto = "fat"`, `codegen-units = 1`, `debug = 1` (line tables for `perf`), and
`panic = "unwind"` pinned explicitly so nobody "optimises" away the per-session crash
isolation.

**Not done, deliberately.** `-C target-cpu=native` is the obvious next win for the
capture/convert paths — but it would bake this machine's ISA into a binary the flake also
packages for other machines. Needs a decision on whether the Nix package and the local
daemon are allowed to diverge before it goes in `.cargo/config.toml`.

**What answers it.** Build both, run the known-good baseline (1728×1080 @120, 12000 kbps,
`vaapi-dmabuf`), compare `tick` and `encode` from `browser: latency`. If native is worth
under a millisecond it is not worth the portability split.

---

## O6 — A receiver-side stall inflates the jitter buffer, and the reclaim does not work

**Measured on the phone (1080×2422 @ scale 2.0, 120 fps, Quality), 2026-09-11 18:31.**

A single hitch — `fps=46`, `framesDropped` +482 in one 5 s window — took `jbuf` from 36 ms to
69 ms. It then decayed **17 ms over 60 s (~0.28 ms/s)** while `rtt` stayed 17–31 ms and
`lost=0` throughout. The hitch lasts seconds; the latency penalty lasts minutes. That
after-effect, not the hitch, is what is felt.

**The existing reclaim (`W.reassertPlayout`) fires and is ignored.** 15 reasserts, every 4 s,
each setting `jitterBufferTarget=20`, across a window where `jbuf` went 56→55→54→54→53→52 —
i.e. exactly the natural drain rate, with no inflection. This closes the v0.0.2 changelog item
"jitter-buffer reclaim — shipped, not yet confirmed by a human": **it is confirmed not to
work.**

**Do NOT build the fix the code comment proposes** ("step the target down gradually").
`jitterBufferTarget` is a floor the browser honours *up to* what its own timing model demands;
playout ≈ max(target, model). If the model wants 52 ms, no value of the hint — set once or
stepped — changes anything. Stepping is a variation on what already failed.

**Two live hypotheses.**
- **(A) Timing model inflated.** The browser's estimator grew on the hitch and decays on its
  own schedule. The hint is not binding.
- **(B) Decoder backlog with no headroom.** At 120 fps the phone has 8.3 ms per frame. If mean
  decode time is at or past that, a one-off stall puts N frames of backlog in the queue and
  the receiver never gets slack to drain it — it only leaks out through occasional skips.
  Predicts the effect is worse at 120 than 60, and points at adaptive fps, not a playout knob.

**The discriminating measurement (deployed 2026-09-11, awaiting data):** `stats.js` now logs
`jtarget` (`jitterBufferTargetDelay / jitterBufferEmittedCount`) and `dec`
(`totalDecodeTime / framesDecoded`), and `minimizePlayoutDelay` logs the `jitterBufferTarget`
read-back.
- `jtarget ≈ jbuf` while the hint reads 20 → **(A)**: model binds, all hint work is dead.
- `dec ≥ 8.3 ms` at 120 fps → **(B)**: no per-frame headroom, backlog cannot drain.
- Both are possible at once; they are not exclusive.

⚠️ **RETRACTED before it spread:** a Reactivity-vs-Quality phone comparison drawn from the
18:29 and 18:30 sessions. The 18:29 session already showed `framesDropped=2326` at its *first*
sample — it was observed post-inflation, which is why it read "flat 27–29 ms", while the 18:30
session was sampled from a clean 13 ms start. **Two phases of one curve, not two preset
behaviours** — the same content/phase confound as O3. A clean phone A/B needs both presets
sampled from connect with no prior stall in either.

**Scope:** this is receiver-side. O1/O2 are server-side. Nothing measured so far connects
them, and O6 is **not** an answer to "app bursts inside the compositor cause jitter".

---

## O5 — Tokio worker count — still rejected

Capping worker threads was considered and rejected for lack of a measured problem, and that
has not changed. `runq_ms` (O2) is what would produce one. **Do not start here.**

---

## Where the knobs live

| Knob | Where | Default | Notes |
|---|---|---|---|
| App CPU share | `WADO_APP_CPU_WEIGHT` env | `50` | `0` disables the scope entirely |
| Release profile | workspace `Cargo.toml` | fat LTO | `panic=unwind` is load-bearing |
| Daemon launch | `scripts/daemon.sh` | release only | the one supported way to start it |


---

## O6 update — the first clean windowed phone trace (2026-09-12)

Session: 1080×2422 @60, scale 1.75, vaapi-dmabuf, real network (rtt 22–53 ms). All three
metrics windowed (0da055f, 20d8d4a), so this is the first phone trace that is not a session
mean.

| t+ | kbps | jbuf | jtarget | dec |
|---|---|---|---|---|
| 0s | 42 | 8 | 11 | 9.58 |
| 10s | 44 | 8 | 11 | 10.03 |
| 20s | 43 | 7 | 11 | 9.69 |
| 25s | 1294 | 15 | 22 | 9.44 |
| 35s | 9906 | 19 | 25 | 9.66 |
| 50s | 5643 | 25 | 30 | 10.67 |
| 60s | 4137 | 23 | 31 | 9.58 |

### The "progressive monotonic ramp" is WITHDRAWN

jbuf went 8 → 9 → 8 → **6** → 7 → 15 → 22 → 19 → 22 → 23 → 25 → 23. It dips, and it rises
only when `kbps` rises (42 → 1294 → 9906). It tracks **content**, not elapsed time. The
earlier "monotonic 10→34 ramp" was read off the cumulative mean, which can only ever go
smoothly up — exactly the artifact 20d8d4a was written to remove.

The "one hitch costs a minute of latency" decay is still unresolved: no hitch occurred here.

### Hypothesis A holds, and the lead has flipped

`jtarget` now runs **7–8 ms above** `jbuf` (31 vs 23) where on the desktop session it ran
2 ms below (24 vs 26). Either way jbuf tracks jtarget, and jtarget is the browser's own
timing model — `jitterBufferTarget` reads back as set and changes nothing. **Every
playout-hint approach is dead.** `reassertPlayout` in webrtc.js is still there and still
does nothing; it should be deleted.

### ~~decode is ~60% of the frame budget and probably explains R1~~ — **WITHDRAWN, same day**

The claim was: `dec` ≈ 9–10.7 ms on the phone is a floor, the 120 fps budget is 8.33 ms,
therefore decode is what stops 120. **Wrong.** `dec` is not a floor — it scales with *bits per
frame*, and bits per frame falls as fps rises at a fixed bitrate.

Measured properly, two sessions minutes apart on the same phone with **only fps different**
(1080×2422, 5676 kbps, Ultrafast, scale 2.0):

| fps | kbit/frame | `dec` observed | mean |
|---|---|---|---|
| 60 | 5676/60 = **94.6** | 8.90 – 10.94 ms | ~10.0 |
| 90 | 5676/90 = **63.1** | 5.70 – 7.80 ms | ~6.9 |

63.1/94.6 = 0.667; 6.9/10.0 = 0.69. `dec` is very nearly **proportional to bits per frame**,
not a fixed cost of the resolution. Extrapolated to 120 fps: 47.3 kbit/frame → `dec` ≈ 5 ms
against an 8.33 ms budget, which fits comfortably.

**The 120 fps row arrived 2026-09-12 20:46, and the extrapolation was too generous.** Same
config again — 1080×2422, 5676 kbps, Ultrafast, scale 2.0 — four windowed samples:

| fps | kbit/frame | `dec` observed | mean | proportional prediction |
|---|---|---|---|---|
| 60 | 94.6 | 8.90 – 10.94 ms | ~10.0 | — |
| 90 | 63.1 | 5.70 – 7.80 ms | ~6.9 | 6.7 ✓ |
| 120 | 47.3 | 6.79, 7.00, 8.39, 7.14 ms | ~7.0 | 5.0 ✗ |

**So `dec` is proportional down to ~63 kbit/frame and then flattens at ~7 ms.** 120 fps gets 25%
fewer bits per frame than 90 and decodes no faster. That is a fixed per-frame cost — the
decoder's own setup, not the bitstream — and it is the floor the earlier "decode is a floor"
claim was groping at with the wrong number.

**What it means for headroom.** Against each rung's budget: 60 fps spends 10.0/16.7 = **60%**,
90 spends 6.9/11.1 = **62%**, 120 spends 7.0/8.33 = **84%**. Decode is *tightest* at 120, and it
stops getting cheaper exactly where the budget stops getting looser. It still fits, so decode is
not what breaks 120 — but there is no longer room to argue 120 is free.

**120 held in this session** (fps 119, 120, 109, 120) and `framesDropped` grew 5 → 5 → 59 → 80
only across the sample where `kbps` burst to 10792. Drops track the bitrate burst, not the frame
rate.

⚠️ **Residual uncertainty, stated because this series has been wrong twice.** The three rows come
from three different sessions minutes-to-an-hour apart on the same phone. The
`compositor session active` lines match on every field, but network conditions and on-screen
content did not hold still, and `rtt` ranged 22–29 ms here against unknown values then.

**So decode does not explain R1**, and raising fps does not make decode harder — it makes each
frame cheaper to decode. The 720×1614@120 drops need another cause.

How the wrong version got written: the first 60 fps trace was taken at 12000 kbps / Veryfast /
scale 1.75 and compared against 90 fps at 5676 / Ultrafast / scale 2.0 — **three variables
besides fps**. That is the content/config confound for the fifth time. The table above is the
matched pair that should have been taken first. Rule that keeps being relearned: **never read
`dec` or `kbps` across two sessions unless the `compositor session active` lines are identical
except for the one variable under test.**

### 90 fps holds on this phone

fps 88–91 sustained across 20 samples, `framesDropped` flat outside two hitches, `dec` ~6.9 ms
against an 11.1 ms budget. The 90 rung (a0ee6ec) is the right ceiling for this device on this
evidence, and 120 remains untested at matched settings.

### "One hitch costs a minute of latency" is WITHDRAWN too

Two hitches captured in one session, both on windowed metrics:

| | fps | jbuf | drops | dec |
|---|---|---|---|---|
| 19:42:27 | **133** (backlog burst) | **203 ms** | +339 | 4.17 |
| next sample | 89 | **36 ms** | +0 | 7.32 |
| 19:43:33 | 60 | **211 ms** | +6 | 12.57 |

jbuf went past 200 ms and was back to 36 ms **within one 5 s sample**, twice. It does not take
minutes to drain. What does persist is small: jbuf ran 31–33 ms before the first hitch and
36–41 ms after, so roughly **+5 ms of permanent inflation per hitch** — real, worth fixing,
and nothing like the "minutes of sluggishness" the retracted v0.0.2 changelog entry claimed.

The fps=133 sample is the giveaway: the browser rendered a backlog faster than the configured
rate to catch up, which is what a stall-then-burst looks like from the receiving end.

---

## O7 — `zwp_linux_dmabuf_v1` is in; nobody has checked a client takes it ⚑ NEXT

**Landed `fa3a1f3`.** Until now `wl_shm` was the only buffer path a client had, so a GPU app
rendered on the GPU, read back to the CPU, wrote shared memory, and the compositor uploaded a
texture again — **two full copies per surface per frame**. At 1080×2422 that is ~10 MB each
way. The global is created per session (its format list needs the renderer) and destroyed with
it; v4 with feedback when the render node is known, v3 otherwise.

**Confirmed in use; the size of the win is still unmeasured.** Chrome takes the path (below),
so the copies described above are genuinely gone. What has *not* been measured is how much that
is worth — the saving lands inside the client and the texture upload, not in the stages
`timing.rs` breaks out, so it may show as lower host CPU rather than a shorter tick. Two things
that could still make it small:

1. Chrome may already be on a software GL path in this session, in which case it has no GPU
   buffer to hand over and shm is what it would pick anyway.
2. The copies that vanish are *inside the client and the texture upload*, not in the stages
   `timing.rs` breaks out. They may land as lower CPU rather than a shorter tick.

**How to answer, in order.**

- [ ] Grep the daemon log for `zwp-linux-dmabuf-v1 advertised` — confirms the global exists at
      all, and which version.
- [x] **The log answers this now, on both branches** (`2f55d82`). A session prints
      `dmabuf path is live — a client is handing over GPU buffers` with the format and modifier
      on its first successful import, and `dmabuf path unused this session — every client buffer
      went through wl_shm` at stop when there was none. Read the next session's log; no `/proc`
      archaeology required.
- [x] **ANSWERED 2026-09-12, first session on the new build.** Chrome hands over `AB24`
      (ARGB8888) under modifier `144115188348910340`, whose top byte is `0x02` = AMD — a tiled
      vendor modifier, so the buffer never round-trips through CPU memory. The path is live.

      **Two dead ends worth not repeating.** Counting `/proc/<chrome-gpu-pid>/fd` for `dmabuf`
      returned **0** while the path was in fact being used — a client does not necessarily hold
      the fd in the process you expect, and it may not hold it at all once the compositor has
      imported it. And the compositor's own count stayed at 3 either way, because those 3 are
      its capture/encode buffers, not clients'. Neither number answers this question. The log
      line does.
- [ ] **Cheapest signal, no `WAYLAND_DEBUG` needed:** count the compositor's own dmabuf fds,
      `ls -l /proc/$(pgrep -x wado)/fd | grep -c dmabuf`. **Baseline measured 2026-09-12: 3**,
      on a live session with the global advertised (v4) and *no* client app launched — those
      three are wado's own capture/encode buffers. If launching Chrome does not move that
      number, Chrome is not handing over GPU buffers and the global is dead weight.
- [ ] Matched A/B: the **same** `compositor session active` line except for the build, Chrome
      as the only client under sustained motion. Compare the per-stage composite time from
      `timing.rs` and host CPU (`/proc/<pid>/stat` utime delta). **Not** `dec` or `kbps` —
      those are receiver-side and say nothing about this.

**Why it matters beyond CPU.** The copies run on the render tick, so they compete with the
frame budget directly — 8.33 ms at 120 fps. This is one of the few remaining items that can
raise the achievable frame rate rather than just the comfort of the one we have.

---

## O8 — decode cost is bandwidth, not resolution: the fps ceiling is the wrong shape

The O6 correction above is not just a retraction, it changes what to tune. `dec` is very
nearly **proportional to bits per frame**. At a fixed bitrate, raising fps *lowers* per-frame
decode cost — 94.6 kbit/frame → ~10.0 ms at 60, 63.1 → ~6.9 ms at 90, extrapolating to ~5 ms
at 120 against an 8.33 ms budget.

**The consequence.** Nothing in the receiver stops 120 fps on this phone. What does bite is
that the same bitrate spread over more frames is fewer bits per frame, and
`memory/latency/bandwidth.md` already records 0.016 bits/pixel as starvation. **The real knob
is bits per pixel, and the UI exposes fps and bitrate separately as if they were independent.**

**Actionable.**

- [ ] Compute and log bits/pixel at session start (`kbps * 1000 / (w * h * fps)`) — it is the
      one number that predicts whether a config will look bad, and no log line carries it.
- [ ] Decide whether raising fps should raise the bitrate to hold bits/pixel constant, or
      whether the quality ladder should present bits/pixel directly. Today a user moving
      60 → 120 silently halves their per-frame bit budget and blames the frame rate.
- [ ] 120 fps at matched settings on this phone, to finish the series. It is the only rung
      never measured with the config held still.

---

## O9 — the +5 ms per-hitch inflation: a re-offer is the only lever left

**Known.** Every network hitch leaves the jitter buffer permanently ~5 ms higher (31–33 ms
before, 36–41 ms after; two hitches, one session). It drains its transient within one 5 s
sample, but the floor moves up and stays.

**Every playout-hint approach is dead** (O6, and `0f66d04` deleted the code). `jitterBufferTarget`
is a floor honoured only up to what the browser's timing model demands, and the model is what
is moving.

**The one lever not tried.** The inflated state lives in the receiver, and a receiver is
created per peer connection. `W.connectWebRTC()` already builds a fresh `RTCPeerConnection` per
connect, so a re-offer resets the jitter buffer to its floor by construction. Cost is an IDR
and an ICE round — sub-second, and the session (compositor, apps, windows) survives it
untouched, because the compositor session is not tied to the peer connection.

**Actionable.**

- [ ] Manual first: a "resync" button in the control bar calling `connectWebRTC()`. If jbuf
      drops back to its floor, the mechanism is proven and costs one button.
- [ ] Only then consider automatic — and only on an explicit threshold with a long cooldown.
      An automatic reconnect that fires on a bad link makes a bad link worse, which is the
      failure mode that killed `reassertPlayout`'s cousin.
