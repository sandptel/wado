# Instrumentation — 2026-09-12

Server-side changes whose only job is to make a question answerable. None of them changes
behaviour; all of them exist because a decision was blocked on a number nobody had.

| item | commit | verified? |
|---|---|---|
| bits per pixel on the session line | `9fb5b99` | ✅ unit test; ⏳ live line not yet seen |
| pump timings as percentiles | `62aeaa9` | ✅ two unit tests; ⏳ cannot force a real stall |
| dmabuf verdict on both branches | `2f55d82`, `557d53e` | ✅ **fired live, both branches** |
| fractional-scale bind log | `afc7c80` (info in `2f55d82`) | ✅ fired live at 1.75, 2.0, 2.5 |

---

## Bits per pixel — the number that predicts a starved config

`kbps * 1000 / (width * height * fps)`, now a field on the `compositor session active` line.

Nothing logged it, and it is the one figure that says whether a resolution/frame-rate/bitrate
combination has asked for the impossible. The UI exposes bitrate and fps as **independent**
settings, so moving 60 → 120 silently halves the per-frame bit budget;
`memory/latency/bandwidth.md` records 0.016 bits/pixel as starvation.

Worked example, the config in live use tonight — 1080 × 2422:

| fps | bits/pixel at 5676 kbps |
|---|---|
| 60 | 0.0362 |
| 90 | 0.0241 |
| 120 | 0.0181 |

120 fps sits a hair above the starvation figure at a bitrate that is comfortable at 60. That is
now visible in the log at session start instead of reconstructable from four other fields.

**Verification.** A unit test asserts the value halves when fps doubles, that this config at
120 lands in the 0.01–0.05 band, and that a zero-fps config returns 0.0 rather than dividing by
zero. The live log line has **not** been read yet — the daemon carrying it started at 04:45 and
no session has run since.

## Pump timings — percentiles instead of a censored threshold

The pump previously logged only overruns past 100 ms. That hides the shape of the thing being
measured: a pump at p50 = 2 ms with a rare 400 ms spike and a pump at p50 = 90 ms produce the
same handful of warnings and are indistinguishable in the log. `memory/latency/07` records a
"pattern" read off exactly that censored view which did not survive contact with the full
distribution.

Every frame is now recorded. One line per 300 frames (five seconds at 60 fps) carries
p50/p90/p99/max and the over-budget count. The outlier warning stays — a single 400 ms stall
still deserves its own line with `runq_ms`/`psi_*` attached — but it is no longer the only thing
visible.

**Verification.** Two unit tests: exact nearest-rank percentiles over a known 1–300 ms
distribution, and the discriminating case — 299 frames at 2 ms plus one at 400 ms must report
`p50 = 2.0` and `max = 400.0`. The second is the assertion the old threshold log could never
have made. A real stall **cannot be forced**, so the live path ships unverified.

*One expectation in the first test was wrong when written* — nearest rank on 300 samples gives
index 150, i.e. 151 ms, not 150. The test caught it; the code was right.

## Findings from instrumentation already deployed

### dmabuf: Chrome does take the GPU path

`dmabuf path is live — a client is handing over GPU buffers format=AB24 modifier=144115188348910340`

Modifier's top byte is `0x02` = AMD, so it is a **tiled** vendor modifier — a buffer that never
round-trips through CPU memory. Fired on three separate sessions. R10 closed.

**Two `/proc` measurements that looked like answers and were not**, recorded so they are not
repeated:

- `/proc/<chrome-gpu-pid>/fd | grep dmabuf` returned **0** *while the path was in active use*.
  A client does not necessarily hold the fd in the process you expect, or at all once the
  compositor has imported it.
- The compositor's own count stayed at **3** either way — those three are its capture/encode
  buffers, never a client's.

Neither number answers the question. The log line does.

### The verdict had a hole, found within a minute of shipping

The first `dmabuf path unused this session` came from a session nobody launched an app into: no
buffers of any kind were produced, so "unused" meant *"nothing asked"*, not *"everything chose
shm"*. Fixed by carrying the mapped-window count, so `windows=0` reads as vacuous on sight
(`557d53e`).

Generalised rule, now in `memory/shared/environment.md`: **a verdict that only logs "yes" is
indistinguishable from nobody looking.** Both branches must speak.

### `runq_ms`: one sample, not a conclusion

```
write_sample stall took_ms=162 runq_ms=0 psi_cpu=0.0 psi_mem=0.0 bytes=11638 packets=9
```

`runq_ms = 0` is consistent with "not CPU starvation", which would demote
`WADO_APP_CPU_WEIGHT` and close the O1/O2 branch point.

### ⚠️⚠️ n = 4 — the fourth sample does **not** fit, and partly withdraws the reading below

```
write_sample stall took_ms=125 runq_ms=38 psi_cpu=0.0 bytes=25012 keyframe=false packets=20
```

Two things make this one different from the three below:

- **`runq_ms=38`, the first non-zero.** Roughly a third of the 125 ms was scheduling delay, not
  transfer. The "runq is always 0, therefore not CPU" reading is now 3-of-4, not 4-of-4.
- **200 kB/s effective — 2.3–2.9× the others.** Excluding the run-queue time it is ~287 kB/s. It
  sits nowhere near the 69–88 kB/s line that made the rate-limited-writer story look strong.

**Sample 5, one minute later, same config:** `took_ms=145 runq_ms=0 bytes=25012 packets=20` —
byte-for-byte the same payload size as sample 4, 16 % slower, and with *zero* run-queue time where
sample 4 had 38 ms. So the run-queue delay did not even make its own sample the slower of the two.
Whatever `runq_ms` is measuring here, it is not the thing that sets `took_ms`.

Grouped by config rather than pooled, which is the only honest way to read them:

| config | samples | effective rate |
|---|---|---|
| 5676 kbps, Ultrafast | 3 | 69–88 kB/s |
| 12000 kbps, Veryfast | 2 | 172–200 kB/s |

Within each group there is too little spread in payload size to fit anything, and across groups the
comparison is invalid. **Five samples, no conclusion** — that is the state, and it is a better state
than the three-sample line that looked like one.

**What is worth saying to a human right now** is the plain operational reading: at 12000 kbps the
stalls are on **ordinary frames**, not keyframes. That is the shape of a bitrate the link cannot
carry, and it is checkable in one session by pinning the bitrate down.

### The 12000 kbps group, with payload held constant — this is the clean measurement

Four stalls at `bitrate_kbps=12000`, **all of them `bytes=25012 packets=20`**:

| took_ms | 120 | 125 | 145 | 431 |
|---|---|---|---|---|
| runq_ms | 0 | 38 | 0 | 0 |
| effective | 208 kB/s | 200 kB/s | 172 kB/s | 58 kB/s |

**Identical payload, identical config, and a 3.6× spread in duration.** That is the measurement the
earlier three could not make, and it settles the shape: with bytes held constant the duration still
varies by 3.6×, so `took_ms` is **not** a function of payload size. The variance lives in the link,
not in the bytes — which is what a mobile uplink looks like and is not what a fixed-rate writer
looks like.

**Why every frame is exactly 25012 bytes** is worth naming, because it is not a coincidence:
12000 kbps ÷ 60 fps = 200 000 bits = **25 000 bytes per frame**. CBR (invariant 7) is hitting its
per-frame budget exactly, on every P-frame. So the encoder is asking the link for a steady
1.5 MB/s, and the write path is draining at 58–208 kB/s when it stalls — **one order of magnitude
short**.

That is as close to a direct answer as this instrumentation can give: **12000 kbps is far above
what this link carries.** The bitrate-down test is no longer a hypothesis to check, it is a
confirmation to run.

### 07:23:04–07:23:06 — a burst, and it is the most informative thing yet

Six stalls inside **1.3 seconds** of wall time:

| took_ms | bytes | packets | runq_ms | effective |
|---:|---:|---:|---:|---:|
| 120 | 25 012 | 20 | 0 | 208 kB/s |
| 165 | 16 453 | 13 | 0 | 100 kB/s |
| 157 | 42 614 | 35 | 0 | 271 kB/s |
| 266 | 28 264 | 23 | 0 | 106 kB/s |
| 162 | 25 012 | 20 | 0 | 154 kB/s |
| 111 | 25 012 | 20 | **80** | 225 kB/s |

**981 ms of stall inside 1.3 s.** The write path was blocked roughly **75 % of that window** — this
is not a tail event, it is the pipeline failing to keep up, and the viewer was watching it happen.

Two things to carry forward:

**1. `runq_ms=80` in a 111 ms stall — 72 % of it.** The largest by far, and it flatly contradicts
the "runq is always 0, therefore not CPU" reading that three early samples supported. Note the
contradiction inside the *same line*, which is not resolved: `runq_ms=80` says 80 ms runnable but
not scheduled, while `psi_cpu=0.0` on the same sample says no CPU pressure at all. One of those two
is measuring something other than what its name suggests. **Do not build on either until that is
settled** — and note that `runq_ms` should not inflate while a task is blocked on I/O, because a
blocked task is not runnable.

**2. The rate is not stable even within one burst:** 100–271 kB/s across 1.3 seconds, on payloads
from 16 kB to 43 kB. There is no single "link rate" to fit. Whatever is happening is bursty at a
sub-second scale, which is what a congested mobile uplink does and what no amount of averaging will
describe.

**What I am withdrawing:** "the stall duration is proportional to the payload at a flat rate" is a
claim about three samples that a fourth contradicts. It may still be right for *a* population of
stalls — the three tight ones — but it is not a description of stalls in general.

**A confound worth naming before anyone re-reads this.** The session config changed between the
samples: `bitrate_kbps` went 5676 → **12000** and the preset Ultrafast → Veryfast while the user
was testing. Sample 4 is from the 12000 config. So "the rate changed" and "the link changed" and
"the encoder is producing very different frames" are all live, and these four samples were **not**
gathered under matched conditions. That is exactly the comparison-across-configs mistake
`memory/latency` already records five times.

The pinned-bitrate test proposed below is now more necessary, not less: it is the only way to get
samples that can legitimately be compared.

### n = 3 (superseded above), and the three of them line up on *bytes*, not CPU

| took_ms | bytes | packets | keyframe | runq_ms | psi_cpu | effective rate |
|---:|---:|---:|---|---:|---:|---:|
| 162 | 11 638 | 9 | false | 0 | 0.0 | 71.8 kB/s |
| 111 | 9 789 | 8 | false | 0 | 0.02 | 88.2 kB/s |
| 732 | 50 571 | 42 | true | 0 | 0.0 | 69.1 kB/s |

**The stall duration is proportional to the payload**, across a 5× spread in size, at a
strikingly flat **69–88 kB/s ≈ 550–700 kbit/s**. The session was configured at **5676 kbps**, so
during these moments the write path moves at roughly **one eighth** of the bitrate the encoder is
being asked to produce.

That is the signature of a **rate-limited writer**, not a scheduling hiccup. A scheduler stall
would put `took_ms` and `bytes` in no particular relation; these sit on a line. The mechanism that
fits: a saturated uplink fills the UDP socket buffer, the non-blocking `sendto` returns
`EWOULDBLOCK`, and tokio waits for writability — which drains at whatever the link can actually
carry. `environment.md` already records this WAN at 27–50 % UDP loss, and the host is on a phone
hotspot.

**Caveat, stated plainly:** the log only fires above a 100 ms threshold, which selects for large
payloads and *partly* manufactures the correlation — any constant-rate writer would put every
sample on the same line by construction. What the threshold does **not** explain is the ratio
holding steady across 5× in size; random stalls would scatter it. n = 3 is a pattern worth a test,
not a closed question.

**The test it suggests** is different from the VBV A/B already queued: pin the bitrate *down* —
1500 kbps at the same resolution and fps — and see whether these stalls disappear entirely. If
they do, the open pixelation question is a bitrate-above-uplink problem, and the encoder settings
are not where the fix lives.

**n = 2 as of 06:42 UTC** — a second stall on the new daemon says the same thing:

```
write_sample stall took_ms=111 runq_ms=0 psi_cpu=0.02 psi_mem=0.0 bytes=9789 keyframe=false packets=8
```

Two stalls, both ≥100 ms, both with **zero run-queue delay and essentially zero CPU pressure**.
That is consistent with "not CPU starvation" — which would demote `WADO_APP_CPU_WEIGHT` and close
the O1/O2 branch point — and it is still **not a finding**. `optimisation.md` O2 asks for volume,
and two samples from two sessions is not volume. What it does do is stop the hypothesis looking
like a single fluke. The percentile line is what should produce the rest.

---

## Still open

- Read the `bits_per_px` line on a real session, and add it to the O8 table so future traces
  carry the figure rather than three fields to multiply.
- **Deferred with reason:** the dmabuf *cost* question (O7) needs a matched A/B against the
  `checkpoint-pre-dmabuf` tag — a second release build of an old commit, and the saving lands in
  host CPU rather than in any stage `timing.rs` breaks out. Not worth a 3-minute build and a
  daemon swap while the machine is under test.
- **Deferred with reason:** the build-starvation theory (R2) requires deliberately saturating
  20 cores during a live session. Antisocial right now.
- **Deferred with reason:** the VBV A/B that would settle R1 needs a live session with the
  bitrate pinned, i.e. a human driving the client.
