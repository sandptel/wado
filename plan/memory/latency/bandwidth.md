# bandwidth — how much the stream actually needs

## The unit that matters is pixel rate, not resolution

Demand scales with `width × height × fps`. Resolution alone is misleading: the phone in
portrait at 120 fps is a **heavier** load than desktop 1080p120.

| config | Mpx/s | light use | scrolling / video |
|---|---|---|---|
| 1280×720 @60 | 55 | ~3 Mbps | ~6 Mbps |
| 1728×1080 @60 | 112 | ~6 Mbps | ~11 Mbps |
| 1920×1080 @60 | 124 | ~6 Mbps | ~12 Mbps |
| 1728×1080 @120 | 224 | ~11 Mbps | ~22 Mbps |
| **1080×2422 @120** (the test phone) | **314** | ~16 Mbps | ~31 Mbps |

Those are 0.05–0.10 bits/pixel, the normal band for H.264 at ultrafast/veryfast with no
B-frames. **Ultrafast is deliberately inefficient** — it buys latency with bitrate — so this
runs higher than a file-encoding rule of thumb. External anchor: Moonlight defaults to
~20 Mbps for 1080p60 games.

⚠️ The bits/pixel band is a **rule of thumb, not a wado measurement**. The pixel-rate
arithmetic is exact; the coefficient is not. A controlled quality-vs-bitrate sweep has never
been run here.

## Measured: the live stream is bandwidth-starved at 120 fps

Live sessions 2026-09-11 never exceeded **~8.2 Mbps** whatever was asked for. On the phone at
1080×2422@120 that is **0.016 bits/pixel** — three to six times under the band above. Detail
collapsing whenever anything moves is the expected consequence, not a bug to hunt.

**The lever is fps, not bitrate.** Halving 120 → 60 halves pixel rate, so the same bitrate
becomes twice the bits per frame. At 314 Mpx/s the bandwidth is being spent on frame count
instead of picture.

## The preset labels are a cap, not a delivery rate

Both of these are true at once, and saying only the first is dismissive of what the user sees:

- **The labels are honest.** Given demanding content the encoder *overshoots* every target —
  111% / 115% / 117% at 4050 / 8100 / 12000. See `optimisation.md` O3b. Do not relabel.
- **Switching Balanced → Quality on an idle desktop changes nothing visible**, because CBR has
  no bits to spend when nothing moves. The number is a ceiling the encoder is allowed to reach,
  not a rate it will produce on demand.

## Trap

**Never compare delivered bitrate across configurations that differ in more than one thing.**
This confound has now been made three times in one session — across live sessions (O3), across
idle vs busy content (O3b), and across `testsrc2` 1728×1080@60 vs live phone 1080×2422@120.
Two throughput numbers are comparable only at the same resolution, fps, keyframe interval and
content.

## ⚑ The preset ladder collapses at tall phone resolutions (`2026-09-13`)

The presets name a **bits-per-pixel budget at 720p** and scale by pixel count, with a hard
`CEILING_KBPS = 12_000` because webrtc-rs 0.17 has no congestion control on its send path.

At 1280×720 that is a clean 4× ladder. At the output wado actually creates for this phone —
**1080×2422** — the ceiling squashes the top two together. Read off the daemon's own
`compositor session active` lines, not computed:

| preset | 720p | 1080×2422 @ 90 | GOP | x264 preset | bits/px |
|---|---|---|---|---|---|
| Reactivity | 2000 | **5676** | **90 (1 s)** | Ultrafast | 0.0241 |
| Balanced | 4000 | **11353** | 180 (2 s) | Ultrafast | 0.0482 |
| Quality | 8000 | **12000 — clamped** | 180 (2 s) | Veryfast | 0.0510 |

Two consequences that matter more than the numbers:

1. **Balanced and Quality are 6% apart and otherwise identical.** `preset` is read only in
   `x264enc.rs` — the VAAPI encoder never looks at it — so on hardware `Ultrafast` vs `Veryfast`
   is a no-op. Quality is not "more quality" here; it is Balanced plus 647 kbps, clamped.
2. **Reactivity is the only preset that changes the GOP.** One second against two. On a link
   that loses packets, GOP length dominates *perceived* stability far more than bitrate does,
   because it sets how long a glitch lasts.

Measured on the same session, for scale: the link delivered **p50 2.6 Mbps, p90 4.5 Mbps, max
8.8 Mbps**. Reactivity asks 5.7; Balanced and Quality ask ~2.5× the p90. Encode cost was **p50
5.1 ms, p90 6.2 ms** per frame and is essentially preset-independent — VAAPI cost tracks
pixels × fps, not bitrate.

**So at phone resolutions the useful choice is Reactivity or an explicit Custom bitrate.** The
ladder needs rethinking above ~2 Mpx: either the ceiling scales, or Quality earns its name some
other way (a shorter GOP would do more for this link than 6% more bits).
