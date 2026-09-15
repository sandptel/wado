# latency — encoder

## Bitrate scales with resolution (since 2026-09-11)

A preset names a bits-per-pixel budget anchored at 1280×720; the target follows pixel count,
clamped to [1000, 12000] kbps. 720p is unchanged to the kbps.

|  | Reactivity | Balanced | Quality |
|---|---|---|---|
| 1280×720 | 2000 | 4000 | 8000 |
| 1728×1080 | 4050 | 8100 | 12000 |
| 1080×2422 | 5676 | 11353 | 12000 |

**fps is deliberately absent from the formula** — doubling fps does not double the bits
needed for equal quality, and scaling on it asks 32 Mbps at 1080p120.

**The ceiling is a network decision, not an encoder one.** There is no congestion control in
the send path (see `transport.md`), so an uncapped target is a promise the link never agreed
to: fine on a LAN, packet loss on cellular.

A `Custom` bitrate is passed through unscaled — a typed number means that number.

## Why flat bitrate looked like the opposite of what it was

Reported as "1080p Balanced and Quality unusable, Reactivity fine" — i.e. the *lowest*
bitrate preset was the usable one. Not a bitrate paradox: Reactivity uses a **1-second GOP**
and the others a 2-second one, so it recovered from damage twice as fast. The damage itself
came from the debug build.

## VBV is sized in BITS, not frame times (fixed 2026-09-11)

`VBV_MILLIS = 67` (four frame times at 60 fps, where it was tuned), clamped by
`VBV_MAX_BYTES = 65_536`. **VAAPI/ffmpeg path only — x264 is configured with a bitrate and no
VBV at all**, which is why hardware-specific symptoms appear here.

**The bug it fixed:** counting in frame times made the allowance depend on frame rate, so
1080p at 120 fps got *half* the bits per keyframe that 60 fps got — 16.5 KB for a 1728×1080
IDR. CBR can only hit a cap that small by raising the quantiser, so every keyframe came out
blocky and cleaned up over the following frames: a **once-per-second pulse at the IDR
cadence**. Reported as *"the screen is flashing, slight pixelated ↔ proper quality"*.

**The lesson, which generalises:** what stalls the pump is the size of a single write, and
that is measured in **bits**. Any knob defending against it must hold bits constant. A
per-frame unit silently rescales with frame rate.

The 64 KiB cap is empirical, not chosen: it is the largest burst measured clean on release
(1728×1080 @60, 8000 kbps — zero stalls, worst queue wait 0 ms). Raising it needs a
measurement.

**No timing metric shows this fault.** fps, jbuf, stalls and queue wait were all healthy
throughout. It is only visible on screen — which is why the pump now logs keyframe count,
avg/max keyframe KB and avg P-frame KB per stretch.

## The VBV increase has a bounded, accepted cost at 120 fps

A 65 KB keyframe takes ~10 ms to write against an 8.3 ms frame slot, so `write_sample slower
than the frame budget` now fires occasionally on IDRs at 120 fps — it must, since a keyframe
is ~8x a P-frame. Measured: 60 slow frames in a session, worst 81 ms, **zero stalls (>100 ms),
zero drops, fps 120-121, jbuf 20 ms flat**. The 2-slot channel and the jitter buffer absorb it.

**Do not "fix" this by lowering the VBV** — that is the flashing bug coming back. Act only if
stalls or drops appear with it, and note that the cap shapes averages rather than bounding any
single frame (80 KB and 109 KB keyframes have been observed).

## Invariants

- CBR, no B-frames, no lookahead, keyframes **on demand** not periodic. Intra-refresh is
  unavailable in both encoders, so on-demand IDR is how the goal is met.
- VBV is a **latency** knob, not a quality one: `VBV_FRAME_TIMES = 4`.
- H.264 default. H.265/AV1 only with advertised *hardware* decode.
- Hardware encoders are detected **by opening them** and caching the result.
- Software-encode fallback must tell the user, in UI and log.

## Tier cost, measured (1728×1080, same machine, 2026-09-11)

Only the encoder tier differs. Milliseconds, from the browser's per-stage breakdown:

| stage | `vaapi-dmabuf` | `x264-cpu` | |
|---|---|---|---|
| capture | 0.2 | 2.1–3.2 | **~13×** — hardware is DMA-BUF zero-copy; software reads pixels back to host memory |
| encode | 2.6–2.9 | 7.0–9.0 | ~3× |
| tick | 8.3 (stable) | 14.1–20.1 (variable) | the ceiling |
| buf (jbuf) | 12.3 | 21.9–23.1 | |
| decode | 2.5 | 2.8 | tier-independent, as expected |

**A tick of 14–20 ms is a 50–70 fps ceiling**, which is exactly the observed 62–116 fps wobble
when 120 was asked for. The 120 fps budget is 8.33 ms — software cannot meet it at 1080p.

Not a transport problem: queue 0.0, zero stalls, zero drops, zero loss throughout. Purely
encoder-bound.

Rule of thumb: **software tier is fine at 720p, marginal at 1080p60 (tick ~17 ms vs a
16.7 ms budget), and cannot do 1080p120.** The UI currently lets you ask for a combination
the software tier cannot deliver, and nothing says so before you try.

## Open

- Keyframe cadence: a decision is documented in `x264enc.rs` and has never been made.
- Should the client cap fps/resolution options, or warn, when the software tier is active?

---

## Balanced is NOT anomalous — earlier claim WITHDRAWN (2026-09-11)

An earlier entry here claimed Balanced emitted bigger frames than Quality at lower bitrate,
and blamed the absolute 64 KB VBV cap. **Both the observation and the explanation were wrong.**

**The error:** `key_max_kb` was compared across different live browsing sessions, i.e. across
different screen content. That measures the web page, not the encoder.

**Controlled** (identical `testsrc2`, 1728×1080@60, only bitrate differing) — monotonic:

| preset | kbps | IDR avg | IDR max | P avg | IDR ÷ VBV |
|---|---|---|---|---|---|
| Reactivity | 4050 | 27.9 KB | 36.8 KB | 9.0 KB | 1.11× |
| Balanced | 8100 | 43.0 KB | 50.8 KB | 18.8 KB | 0.79× |
| Quality | 12000 | 52.7 KB | 58.1 KB | 28.3 KB | 0.91× |

IDRs sit at or below the VBV, so **radeonsi does honour `rc_buffer_size`** — another thing
previously doubted. The VBV knob works as designed.

⚠️ **Never compare encoder behaviour across live sessions.** Use `testsrc2` and hold content
fixed. The repro command is in `plan/optimisation.md` O3.

## Hardware and driver this was all measured on

**AMD Radeon 880M (Strix), radeonsi, Mesa 26.2.2, libva 2.24 / VA-API 1.24, ffmpeg 8.1.2.**
Not Intel iHD. Every VBV, bitrate and preset finding in this file was measured on this one
driver and is **unverified anywhere else**. A cheap cross-check when it matters: run the same
A/B through `libx264` with the same `-bufsize` — x264 enforces VBV strictly, so if x264 and
radeonsi agree the behaviour is H.264/parameters; if they differ it is the driver.

