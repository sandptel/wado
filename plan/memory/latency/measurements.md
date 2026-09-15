# latency — measured results by configuration

Every row came from a real session log, not from recollection. Extracted with the aggregator
described at the bottom.

---



## ⚑ Read the decoder's **duty cycle**, not its decode time

`dec` is mean decode time per frame decoded, so `fps × dec / 1000` is the fraction of each second
the decoder is busy. Measured `2026-09-12`, 95 samples, one phone, one resolution:

| config | mean duty |
|---|---|
| `1080×2422 @60` | **0.50** — half idle |
| `1080×2422 @90` | **0.77**, and **0.94** sustained through a collapse |

Through the collapse, `fps × dec` was constant to within a few percent for twenty consecutive
seconds (fps 21–44, dec 44.5–20.4, duty 0.90–0.97). **That is a saturated resource.**

⛔ **A `dec` above budget does not mean the decoder slowed down.** Once saturated, `dec` is the
reciprocal of the achieved frame rate and carries no extra information — `dec` and `fps` become
one measurement reported twice. Two explanations were built on the wrong reading and both are
**withdrawn**: thermal throttling, and a rival app taking the decoder. Neither was needed.

It also explains why one session held for eighty seconds and another collapsed immediately:
starting just under budget accumulates backlog slowly, starting over it does not.

⚠️ **`fps × dec` can legitimately exceed 1.0**, so it is a *load ratio*, not a duty cycle in the
strict sense: a multi-threaded decoder decodes several frames at once, and per-frame decode time
is then wall-clock per frame *per thread*. 154% was observed five seconds into a session. Early
samples are also noisy — `fps` and `dec` come from counters over slightly different windows, and
a few hundred frames in, both deltas are small.

What survives that caveat is the empirical observation, which is what matters: through the
collapse the ratio was **pinned flat** across twenty consecutive samples while fps halved and
`dec` doubled. A plateau is the signature of a saturated resource whatever its thread count.

⛔ **The load ratio is only valid on a low-loss link.** `dec` is `totalDecodeTime / framesDecoded`,
and a decoder stalled waiting for packets that never arrive counts that stall as decode time.
Aggregated by config over the roaming run:

| config | n | mean load | max |
|---|---|---|---|
| `@60`, 11353 kbps — **the 16% loss session** | 64 | **2.84** | **8.75** |
| `@90`, 5676 kbps | 89 | 0.82 | 1.97 |
| `@120`, 5676 kbps | 10 | 0.54 | 0.73 |
| the clean collapse window, `lost=0` throughout | 23 | **0.94** | 0.99 |

A mean of 2.84 and a max of 8.75 are not decoder measurements; they are packet-loss measurements
wearing a decode label. **Check loss before reading this number at all.**

The finding above rests on the `lost=0` window and is unaffected. The `@120` figure is 10 samples
across sessions that kept failing — **not enough to conclude anything**, and it is recorded here
only so nobody mistakes its absence for a result.

**Use `duty > 0.9` as the saturation test, on a low-loss link only.** It is unambiguous where `dec > budget` invites a
story about degradation that the data does not support.

## ⚑ fps is the decode constraint on a phone — not resolution, not bitrate

Measured `2026-09-12`, same device, same session hour, both server-clean:

| | `720×1614 @90`, 2521 kbps | `1080×2422 @60`, 11353 kbps |
|---|---|---|
| bits/frame | 28 kbit | 190 kbit (~7×) |
| `dec` | 8.9 ms, collapsing to 23–113 ms | **10.92 ms, steady** |
| budget | 11.1 ms (80% used) | 16.7 ms (**65% used**) |
| client fps | 8–44 | **59–60 of 60** |
| frames dropped | 60–90 **per second** | **1 in 1475** |

Seven times the bits per frame cost only 2 ms more decode, while the budget grew 5.6 ms and
the frame count fell by a third. Consistent with the ~7 ms per-frame decode floor already
recorded here: **per-frame overhead dominates, so frame rate is the lever.**

**Recommendation, and it is counter-intuitive: on a phone dropping frames, cut fps first.**
Higher resolution at 60 beats lower resolution at 90. A prediction that the heavier stream
would decode worse was made before this measurement and is **withdrawn**.

## ⚑ Strong findings and open threads

Things this table says that are worth acting on or researching. Fuller research goals live in
[`../../research.md`](../../research.md).

| # | Finding | Strength | Next |
|---|---|---|---|
| 1 | **Debug vs release is the dominant variable, ahead of every setting.** Same config: release jbuf 12 ms flat / 0 stalls, debug jbuf 19→42 / 6 stalls. No tuning recovers it. | **Proven** — A/B, same config | Done: startup WARN |
| 2 | **The software tier cannot hold 1080p120.** tick 8.3→22.5 ms against an 8.33 ms budget; 62–90 fps. Hardware on the same config: 114–121 fps, tick 8.3–9.2. | **Proven** — A/B, same config | UI should warn or cap — undecided |
| 3 | **Capture costs ~13× more on the software tier** (0.2 → 2.1–3.2 ms), because the zero-copy DMA-BUF path is gone, not just the encoder. | **Proven** | Research: can host-memory capture be cheaper? |
| 4 | **720×1614 @120 dropped 2497 frames; 1728×1080 @120 dropped 0.** Server-side clean (0 stalls, qmax 1). Fault is downstream of the server — most likely the phone's decoder. | **Strong, uninvestigated** | Research target |
| 5 | **Queue spikes to 234 ms with 8 stalls appeared only while the host was compiling.** Client stayed healthy. | **Correlation only** | Deliberate A/B needed |
| 6 | **Portrait phone outputs carry higher jbuf than landscape at equal pixel count** (720×1614: 7→33; 1152×720: 16→31 but 57 drops). Confounded with the network leg. | **Weak** | Needs a controlled run |
| 7 | The 12000 kbps ceiling is reachable at 1080p120 on hardware with no penalty: jbuf 12→13, 0 stalls. | **Proven** | Ceiling looks safe on LAN; untested on cellular |
| 8 | **A fault can be invisible to every metric here.** The 1080p120 keyframe flashing ran with healthy fps, jbuf, stalls and queue wait throughout — it was a *quality* oscillation, and nothing collected measured quality. Found only because a human watched the screen. | **Proven** | Pump now logs keyframe KB; see `encoder.md` |

---

## The table

`req` = requested fps · `drop` = client cumulative framesDropped · `stall` = `write_sample`
stalls · `qmax` = worst pump queue wait (ms) · `tick` = render tick (ms; budget is
1000/fps — **8.3 at 120, 16.7 at 60**)

```
build    tier           output       req   kbps preset      fps achieved    jbuf ms   drop stall  qmax      tick
release  vaapi-dmabuf   1728x1080    120  12000 Veryfast         114–121      12→13      0     0     6   8.3–9.2
release  x264-cpu       1728x1080    120  12000 Veryfast           62–90      15→17      0     0     0  8.3–22.5
release  x264-cpu       1728x1080    120   4050 Ultrafast         50–116      13→25      0     0    26  8.3–20.4
release  vaapi-dmabuf   1728x1080     60   4000 Ultrafast          60–61      12→12      0     0     0      16.7
release  vaapi-dmabuf   1728x1080     60   8000 Veryfast           59–61      12→15      0     0     0      16.7
release  vaapi-dmabuf   1728x1080    120   8000 Veryfast         119–121      11→13      0     0     7  8.3–16.7
release  vaapi-dmabuf   720x1614      60   2000 Ultrafast          59–60       7→14      0     0     0         —
release  vaapi-dmabuf   720x1614     120   2000 Ultrafast          46–51      11→18   2497     0     1         —
release  vaapi-dmabuf   1728x1080    120   8000 Veryfast          74–121      12→15      4     8   234  8.3–22.1   ← host was compiling
debug    vaapi-dmabuf   1728x1080     60   4000 Ultrafast          48–60      19→42      0     6   138 16.6–17.8   ← the reported fault
debug    vaapi-dmabuf   1728x1080     60   8000 Veryfast           46–60      11→35      0     2   123      16.7
debug    vaapi-dmabuf   1728x1080    120   2000 Ultrafast         67–119      18→30      0     0   124  8.3–19.2
debug    vaapi-dmabuf   1152x720      60   2000 Ultrafast          58–61      16→31     57     0    60 16.7–33.3
debug    vaapi-dmabuf   720x1614      60   4000 Ultrafast          56–59      21→48      0     1   158         —
debug    vaapi-dmabuf   720x1614      60   2000 Ultrafast          58–61       7→33     12     1   107         —
debug    vaapi-dmabuf   1080x2422     60   2000 Ultrafast          55–61       8→15      4     0    95         —
debug    vaapi-dmabuf   1728x1080     60   2000 Ultrafast          51–61      26→36      0     2    94      16.7
```

Earlier debug rows at 1728×1080 @60 / 2000 kbps (jbuf 10→16, 11→13, 14→25, 10→26, 22→28,
qmax 4–66) are omitted as repetitions of the same point.

## Post-VBV-fix (2026-09-11, `VBV_MILLIS=67`)

1080×2422 @120, 5676 kbps, `vaapi-dmabuf`, **cellular** (`rtt` 23–50 ms):

| | |
|---|---|
| keyframe size | **45–52 KB** (old cap computed ≤23 KB) |
| P-frame size | 2–6 KB |
| stalls / worst queue | **0 / 0–2 ms** |
| settled client | fps 122, jbuf 15, rtt 23, drops 1 |

**Doubling the keyframe allowance did not bring back pump stalls** — that was the one real
risk in raising a VBV, and it is refuted on a real link.

⚠️ Mid-session this row showed jbuf 31→37 ms and fps 63–118, which looks like the debug-build
signature and **is not**: `rtt` was swinging 25–50 ms. The buffer was tracking network jitter.
Always read `rtt` before calling a climbing jbuf a regression.

## Reading it

- **`fps achieved` vs `req` is the first thing to look at.** A gap means the *producer* is
  behind — encoder or capture — not the network.
- **`tick` above the budget is the proof.** 22.5 ms against an 8.3 ms budget is a ~44 fps
  ceiling arithmetically; no transport fix touches it.
- **jbuf as a *range* matters more than its value.** `12→12` is a healthy pinned buffer;
  `19→42` is a buffer *climbing*, which means delivery is uneven and is the signature of the
  debug-build fault.
- **`drop` with a clean server** (0 stalls, low qmax) points downstream: the client's decoder
  or its display pipeline. That is finding #4.
- **`stall` + high `qmax` together** means the pump is the bottleneck. Either is suspicious
  alone; together they are conclusive.

## Known-good baseline

**1728×1080 @120, 12000 kbps, Veryfast, `vaapi-dmabuf`, release.** 114–121 fps, jbuf 12→13,
zero drops/loss/stalls, qmax 6, tick 8.3–9.2. Use this as the control when A/B-ing anything.

⚠️ All rows are `rtt≈0` LAN except the `720×1614` ones, which include a real cellular leg.
**Never compare across that boundary.**

## How to regenerate

`/tmp/agg.py` parses `compositor session active`, `pipeline selected`, `browser: stats`,
`browser: latency`, `write_sample stall` and `worst_queue_ms` out of a daemon log.

⚠️ **`pipeline selected` is logged BEFORE `compositor session active`.** Attributing it to the
current session labels every row with the *previous* session's tier — which produced a table
claiming the software encoder hit 121 fps. Hold it as pending and apply it to the next session.

---

## Jitter log — 2026-09-11 17:45–17:54 (live session, user testing)

Recorded on the user's instruction ("note details of these jitter and configs for fixing
later on"). Not yet diagnosed. **Do not act on this without the schedstat measurement below.**

### Configs seen, in order

| Time | Output | fps | kbps | Preset | backend | scale |
|---|---|---|---|---|---|---|
| 17:45 | 1728×1080 | 120 | 12000 | Veryfast | Auto | 1.0 |
| 17:49:31 | 1728×1080 | 120 | 12000 | Veryfast | Auto | 1.0 |
| 17:50:48 | 1728×1080 | 120 | **8100** | **Ultrafast** | Auto | 1.0 |
| 17:52:31 | 1728×1080 | 120 | 8100 | Ultrafast | Auto | 1.0 |

All `keyframe_interval=240`, all LAN (`rtt=0`), all release build, tier `VaapiDma`.

### The burst (17:45:49–17:46:02)

| took_ms | bytes | packets | keyframe |
|---|---|---|---|
| 274 | 37078 | 30 | no |
| 442 | 14459 | 12 | no |
| 452 | 100329 | 83 | yes |
| 524 | 19164 | 15 | no |

`worst_queue_ms=300`; client `framesDropped` 0→144, fps 117→79, recovered to 120 unaided.
`lost=0` throughout. Host: load 4.22/20 cores, `MainThread` 59.8%, `wado` 16.3%.

### The two findings that constrain any fix

1. **Duration does not track packet count — settled, n=23.** Over every stall of 200 ms+ in
   the 17:53–18:00 log, `correlation(packets, took_ms) = -0.03`. Not weak: absent.
   The extremes invert the hypothesis outright — the two worst stalls were the *smallest*
   frames (698 ms on 15 packets, 689 ms on 9), while a 145 KB keyframe at 121 packets took
   182 ms. So this is a blocking event of externally-determined length that a frame happens
   to be waiting inside, not per-packet send cost.
   **Consequences:** `us_per_packet` was removed from the log — it was a constant divided by
   an irrelevant denominator. The planned per-packet percentile histogram is retired as the
   first instrument; it would measure an axis now known to be flat. Frame size, bitrate and
   encoder preset are all ruled out as causes.
   **Stalls also cluster in time** (17:53:20 ×2, 17:57:10–17 ×4, 17:58:54–55 ×3), which is
   the signature of a bursty external event rather than a steady cost.
2. **Dropping bitrate 12000→8100 and preset Veryfast→Ultrafast did not remove it.**
   After the change, `worst_queue_ms` still spiked 110/140/188/200/235 across consecutive
   300-frame stretches while `avg_queue_ms` stayed 0–2 and `p_avg_kb` fell to 6–8.
   **Encoder cost and bitrate are therefore both ruled out as the cause.**

### Reading the pump log correctly (cost me a false alarm)

- `slow=` is a **cumulative counter**, not a rate. It climbed 180→540 over ~4 min.
- `worst_ms=` is a **running max for the connection**, not for the stretch. It stayed pinned
  at 524 from the 17:45 burst long after things were healthy.
- `last_ms=` is the only per-event number: it was 9–15 ms against a 7–8 ms budget, i.e. mild
  overrun, while the alarming-looking fields were stale. **Judge severity on `last_ms` and
  `worst_queue_ms`, never on `slow` or `worst_ms`.**

### The measurement that must come first

`/proc/self/task/<tid>/schedstat` field 2 = ns spent runnable-but-waiting on the runqueue.
Sample around each `write_sample`; log the delta with the stall.

- delta ≈ stall duration → scheduling contention. CPU weight / nice on session apps is then
  a real fix.
- delta ≈ 0 → the thread was not waiting for CPU. Priority tuning would be a placebo.

Add `/proc/pressure/cpu` and `/proc/pressure/memory` (`some avg10`) to the same line. Memory
PSI would implicate reclaim (Chrome allocating), which produces exactly this "CPU idle,
thread stuck 400 ms" signature and is invisible to a 1-minute load average.

⚠️ Load 4.22/20 does **not** rule contention out — a 13-second burst cannot move a 1-minute
EWMA. It merely fails to rule it in.

## 2026-09-14 — multi-device pool, connect timings

Rig: 4 daemons under one Remote ID (`WADO_INSTANCES=4`), local relay + cloudflared quick tunnel,
release build. Host link measured clean: 0% loss to router (1.0 ms) and to 1.1.1.1 (5.6 ms).

| what | number |
|---|---|
| join → session → ICE `Connected`, local browser, cold | **1 s** (20:55:48 → 20:55:49) |
| join → `Connected`, phone on a free daemon, first offer | **~1 s** (21:42:50 → 21:42:51) |
| warm rejoin of a running session | **< 1 s** |
| two devices streaming concurrently | phone 1080x2422@90 (d2) + 1670x1080@60 (d4), 21:42 |

**Connect time was never the problem.** The 13 s figure from the first attempt was ~12 s of a
human choosing settings. Every long wait reported during this run was a *refusal-and-retry*
loop, not a slow pipeline — see `plan/memory/shared/pool.md`.

**A failing device is invisible in aggregate counts.** Per-daemon failure totals (d1: 27,
d2: 29) looked like broken daemons and were one device retrying across the pool. Fingerprint
devices by the candidate count in `offer received — N candidates` before attributing anything.

Not yet measured: concurrent *encode* headroom — how many 1080p sessions this GPU sustains
before frames are dropped. That, not RAM, is the real ceiling on pool size.

### Concurrency headroom — 4 daemons, 22:04

Three devices connected at once (three distinct ICE fingerprints: 6-, 8- and 15-candidate),
all four daemons holding live sessions, three of them **1670x1080@120**:

| | |
|---|---|
| load average | **0.95** on 20 cores |
| memory | **4.5 GB of 27 GB** (≈152 MB per idle daemon, plus session) |
| connect time, free daemon | **1-2 s** every time, on every daemon |

So the practical ceiling is **not** CPU or RAM on this host — neither is close. Whatever limits
pool size is concurrent hardware encode, and four 1080p sessions did not reach it. Raising
`WADO_INSTANCES` past 4 needs `WEBRTC_UDP_PORT_MAX` raised with it (100 ports per slice).

Devices seen this run, by offer candidate count: **6** (connects, ~1 s), **8** (connects, ~2 s),
**15** (never established media, ~38 attempts across all four daemons).
