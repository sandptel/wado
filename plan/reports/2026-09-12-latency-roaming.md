# Latency under roaming — actual vs perceived

Collected `2026-09-12` while the user moved across varying mobile networks. Sampler:
`scripts/sample.sh` → `/tmp/wado-rig/samples.tsv`, one row per telemetry line, nothing dropped.

**Status: collection in progress.** Conclusions below are marked ⓘ *established* or ⚠ *provisional*.

---

## Method — and why "latency" needs two numbers, not one

Two quantities get called latency here and they diverge by more than an order of magnitude:

| | what it is | measured by |
|---|---|---|
| **actual (transit)** | how long a frame takes to cross the network | `rtt` on the selected candidate pair |
| **perceived (glass-to-glass)** | finger → pixel, what the user actually feels | `net + buf + decode` plus the server's `capture + encode + queue + tick` |

⛔ **The two clocks must never be added and called end-to-end** — the browser's and the server's
are unsynchronised (`CLAUDE.md`). The breakdown is read as *proportions*, not as a total.

The decisive point, and the reason a ping is a bad proxy for experience:

> **`jbuf` — the receiver's playout buffer — is invisible to `rtt` and is routinely the largest
> single term.** Measured this session at **1081 ms while `rtt` was 84 ms**. A viewer reporting
> "it feels a second behind" and a readout saying "84 ms ping" are both correct.

---

## ⓘ Established: the stage breakdown on a healthy link

From `browser: latency`, `1728×1080 @90`, home WiFi, server and client both clean:

```
capture 0.1 · encode 3.4 · queue 0.0 · tick 11.1 · net ~10.5 · buf 20.0 · decode 8.9
```

| stage | ms | share | whose |
|---|---|---|---|
| capture | 0.1 | ~0% | server |
| encode | 3.4 | 6% | server |
| queue | 0.0 | 0% | server |
| **tick** | **11.1** | **20%** | server — one frame period at 90 fps; this *is* the frame interval |
| net | ~10.5 | 19% | the path |
| **buf** | **20.0** | **37%** | receiver — the browser's playout buffer |
| decode | 8.9 | 16% | receiver |

**Conclusion:** on a healthy link the server contributes ~3.5 ms of avoidable latency. Everything
else is the frame period, the network, or the receiver. **There is nothing left to win on the
server side**, which is consistent with `memory/latency/pipeline.md`'s measured local maximum.

---

## ⓘ Established: fps is the decode lever, not resolution or bitrate

Same phone, same hour, both with a clean server:

| | `720×1614 @90`, 2.5 Mbps | `1080×2422 @60`, 11.4 Mbps |
|---|---|---|
| bits/frame | 28 kbit | 190 kbit (**7×**) |
| `dec` | 8.9 ms → collapsing to 23–113 ms | **10.9 ms, steady** |
| budget used | 80% → far over | **65%** |
| client fps | 8–44 | **59–60 of 60** |
| dropped | 60–90 **per second** | **1 in 1475** |

Seven times the bits per frame cost 2 ms more decode, while the budget grew 5.6 ms and the frame
count fell by a third. Per-frame overhead dominates; decode flattens at a ~7 ms floor.

> **Counter-intuitive rule: on a phone dropping frames, cut fps before anything else. Higher
> resolution at 60 beats lower resolution at 90.**

A prediction that the heavier stream would decode worse was made before this measurement and is
**withdrawn**.

---

## ⓘ Established: how the three bottlenecks are told apart

| reading | bottleneck | why |
|---|---|---|
| loss rate high | **the path** | neither machine is at fault |
| loss ~0, throughput far under target, **fps also down** | **the sender** | never sent. The fps clause is load-bearing — a still screen encodes to ~60 kbps legitimately |
| everything arrived, frames dropped *after* arrival, `dec` at/over budget | **the receiver** | the phone cannot keep up |
| `write_sample` stall, **`runq ≈ took`** | **server CPU starvation** | thread runnable, no CPU |
| `write_sample` stall, **`runq ≈ 0`** | **the link** | thread blocked on the socket; tick-shedding alongside is the correct response, not a fault |

⚠️ **Absence of evidence is not evidence of health.** Three wrong verdicts in one hour all came
from a missing value read as a healthy one. See `plan/memory/shared/verification.md`.

---

## ⓘ Established: congestion control works, and looks alarming while it does

16:39, link offering ~2.7 Mbps against an 11.4 Mbps ask:

```
⚠ SHED   render ticks dropped: 1 in 2, 4 dropped in the window
⚠ PUMP   queue backed up 70 ms — encoder ahead of the network
⚠ PUMP   1/300 over budget  max=87.6 ms
⚠ CLIENT fps=30 lost=0 dec=11.01 ms
⚠ SHED   render ticks dropped: 1 in 1 (recovered)
```

Queue backed up, compositor halved its tick rate, queue drained, full rate resumed — **three
seconds, zero packets lost**. The shed lines are the mechanism working, not a fault.

---

## ⓘ Established: the decoder saturates, and `dec` stops being an independent measurement

**The finding of the run.** `dec` is mean decode time per frame decoded, so `fps × dec / 1000`
is the fraction of each second the decoder spends decoding — its **duty cycle**. Measured across
95 samples:

| config | samples | mean duty | max |
|---|---|---|---|
| `1080×2422 @60`, 11353 kbps | — | **0.50** | 0.51 |
| `1080×2422 @90`, 5676 kbps | 70 | **0.77** | 0.99 |
| …during the collapse window | 23 | **0.94** | 0.99 |

Twenty consecutive seconds at 90 fps, every sample pinned near 0.95:

```
fps=21 dec=44.52 duty=0.93     fps=27 dec=35.19 duty=0.95
fps=22 dec=44.06 duty=0.97     fps=35 dec=26.46 duty=0.93
fps=24 dec=40.32 duty=0.96     fps=44 dec=20.36 duty=0.90
```

**`fps × dec` is constant.** That is the signature of a saturated resource, and it forces a
correction to how every earlier reading here was interpreted:

> ⚠️ **A `dec` above budget does not mean the decoder got slower. Once saturated, `dec` is the
> reciprocal of the achieved frame rate and nothing more** — the decoder is 100% busy, and
> whatever rate it manages, the per-frame time is `1/rate` by definition. `dec` and `fps` are
> not independent evidence at that point; they are one measurement reported twice.

**This supersedes the thermal reading** (below, now withdrawn). Nothing degraded. The decoder has
a fixed capacity of roughly 20–45 fps at this resolution, 60 fps demand leaves it **half idle**,
and 90 fps demand exceeds it — after which the excess is dropped and `dec` merely reports the
leftover rate. It also explains the earlier "eighty good seconds": a session starting *just under*
budget accumulates backlog slowly; one starting *over* it collapses at once.

**Diagnostic rule that follows:** report the **duty cycle**, not the raw decode time. `duty > 0.9`
is saturation and is unambiguous; `dec > budget` is the same fact in a form that invites the wrong
story about the decoder slowing down.

---

## ~~⚠ Provisional: the receiver degrades with sustained load~~ — **WITHDRAWN**

Superseded by the duty-cycle measurement above. The decoder was never degrading; it was pinned at
~95% and the falling frame rate was the consequence, not the cause. Both candidate explanations
(thermal throttling, a rival app) were unnecessary. Retained here because the wrong reading was
acted on — a cold-phone test was recommended to the user on the strength of it.

### Original entry, for the record

`dec` held 8.9 ms for eighty seconds at 90 fps and then collapsed to 23–113 ms and stayed there.
A later session at the same settings was over budget almost immediately **from a cold start**,
which weakens a pure thermal reading — a cold phone should have bought another eighty seconds.

**Unresolved.** Both candidates (thermal/power throttling; another app taking the decoder) are
off-device and invisible to every wado log. The separating test is a cold phone at 60 fps: if
`dec` holds at ~9 ms indefinitely the cause is load-dependent.

---

## Bottleneck conclusions, ranked by what they cost

| # | bottleneck | cost | fix | where |
|---|---|---|---|---|
| 1 | **No feedback loop to the receiver** | 86 s of 15 fps against a 90 fps stream, repeatedly | client-driven fps backoff; hysteresis precedent exists (`SETTLE_TICKS = 3`) | `issues.md` I1 |
| 2 | **Playout buffer ratchets and never drains** | `jbuf` 1081 ms with `rtt` 84 ms — a second of felt lag on a healthy path | ⟳ Resync rebuilds the peer connection and resets it. **Manual today**; the same verdict that names the fault could offer it | new — see below |
| 3 | **fps chosen above what the device can decode** | 60–90 dropped frames/sec | the verdict now says `try 30 fps`; automatic is #1 | shipped `07efb95` |
| 4 | **Bitrate asked far above the link** | 11.4 Mbps requested on a 1.3 Mbps link | no adaptive bitrate; shedding absorbs it lossily | `issues.md` I1, I5 |
| 5 | **Too-small link only detectable after degradation** | lagging indicator | sender-side congestion signal pushed to the client | `issues.md` I5 |

### Suggested fix not yet filed: offer Resync when the buffer has ratcheted

`jbuf` far above `jtarget` on a link that has since recovered is a *specific*, *detectable*, and
*one-tap-fixable* condition — measured at 1081 ms against a 36 ms target. The client already
computes both numbers every second and already has the Resync action. The verdict currently says
what is wrong and what setting to change; this is the one case where it could offer the **action**
instead. Small, and it addresses the single largest felt-latency term in the whole run.

---

## Open data

`/tmp/wado-rig/samples.tsv` — collection continuing. Columns: client fps/rtt/jbuf/jtarget/dec/
kbps/lost/dropped, the seven pipeline stages, server render fps, pump p99 and over-budget count.
Rows tagged `client!`, `render!`, `pump!`, `shed!`, `stall!` are the anomalous ones.
