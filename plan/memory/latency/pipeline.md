# latency — the pipeline

## The measured local maximum (`v0.0.1`, 2026-09-11)

1728×1080 @120 fps, 8000 kbps, `Veryfast`, `vaapi-dmabuf`, **release build**:

| | |
|---|---|
| fps | 119–120 steady |
| jitter buffer | **13 ms, flat** |
| dropped / lost | 0 / 0 |
| `write_sample` stalls | 0 |
| pump queue wait | avg 0 ms, worst 1–7 ms |

**`rtt=0ms` — this is a LAN number.** It covers everything wado controls and nothing it does
not. It is *not* glass-to-glass over cellular, which is what the 80–100 ms target means.
Quote it as "13 ms jitter buffer", never as "13 ms latency".

1080p @60 holds the same on Balanced (4000) and Quality (8000).

## What actually dominated (in order found)

1. **Debug build** — the largest single factor by far. See `shared/environment.md`.
2. **VBV sized for streaming, not latency** — `rc_buffer_size` was one full second of
   bitrate, letting keyframes reach 250–350 KB and stall the pump 110–153 ms. Now four frame
   times (`VBV_FRAME_TIMES = 4`).
3. **Playout delay** — browser buffered ~33 ms until `playoutDelayHint`/`jitterBufferTarget`
   were set. jbuf fell 31–42 ms → 7 ms. Chromium only; Firefox has neither knob.
4. **ICE gathering** — a STUN server erroring kept gathering incomplete past the timeout so
   the offer never went. Capped at 3 s.

## The jitter buffer ratchets — and that is felt as sluggishness

Chrome grows the receiver's jitter buffer in **one step** when the link misbehaves and drains
it at ~0.3 ms per second. A single 200 ms rtt spike took it 8 → 68 ms and it was still at
59 ms four minutes later. The picture is undamaged the whole time; it is simply *late*, which
is what the user reports as sluggishness rather than as stutter.

`jitterBufferTarget` / `playoutDelayHint` are a **target, not a cap**. They are honoured when
set and then overridden by Chrome's own adaptation, and nothing asked again — the hint was
applied once at connect.

**Fix (2026-09-11):** the 1 Hz stats tick re-asserts the hint when the buffer has drifted
past target *and* rtt is already healthy again (`W.reassertPlayout`, webrtc.js). Two-sided on
purpose — forcing the buffer down while the link is still bad trades latency for stutter, and
Chrome grew it for a reason. Logs `playout reasserted` when it fires.

**Interim workaround, proven:** restarting the session resets the buffer (59 → 7 ms). Any
"it got sluggish and stayed sluggish" report should check `jbuf` before anything else.

⚠️ If it fires and jbuf does *not* fall, Chrome is ignoring a repeat set of the same value;
the next step is stepping the target down gradually rather than re-asserting.

## Open

- **Build starvation is unconfirmed.** Stalls of 140–242 ms on 6–12 KB frames appeared
  exactly while `cargo`/`nix` builds saturated 20 cores; the client stayed healthy
  (rtt=0, jbuf 14–15 ms flat, no loss). Plausible, not proved. Test deliberately.
- Input latency has never been read. Needs Debug → latency on.
- `worst_queue_ms` spikes to 200+ under load — is the 2-slot frame channel right?

## Recurring trap

Four relay/direct asymmetries have been found (ICE servers, latency echo, playout delay,
render timings): `relay.js` repeatedly lacks what `webrtc.js` already had. **When touching
one transport path, check the other.**
