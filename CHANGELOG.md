# Changelog

## v0.0.1 — `2026-09-11`

The first tagged point. It marks a **measured latency local maximum**: the settings below
were reached by tracing real sessions, and every number here was read off a running system
rather than estimated.

### The measured state

Session: **1728×1080 @ 120 fps**, 8000 kbps, `Veryfast`, keyframe interval 240, scale 1.0,
pipeline `vaapi-dmabuf` (`tier=VaapiDma`, hardware encode), **release build**.

| Reading | Value |
|---|---|
| Client frame rate | 119–120 fps, steady |
| Jitter buffer | **13 ms, flat** (no upward drift) |
| Frames dropped (client) | 0 |
| Packets lost | 0 |
| `write_sample` stalls | **0** |
| Pump queue wait | avg 0 ms, worst 1–7 ms |

**Read this caveat before quoting the numbers.** They were taken at `rtt=0ms` — a local
path, not the internet. They measure everything wado controls (capture, encode, queue,
packetise, decode) and nothing it does not. They are *not* a glass-to-glass figure over
cellular, and the project's ~80–100 ms target is about that harder number.

Also holding at 1728×1080 @ 60 fps on both 4000 kbps (`Balanced`) and 8000 kbps
(`Quality`): zero stalls, jitter buffer flat at 12 ms.

### What made it possible

**Never run a debug build.** The single largest finding of the session. A `./target/debug`
daemon cannot meet the target and does not fail like a build problem — unoptimised SRTP and
per-packet packetisation produced 100–200 ms pump stalls, a jitter buffer climbing 19→42 ms,
oscillating throughput and frame rate dipping to 48 at 1080p. Every one of those symptoms
reads as a network or encoder fault. The same configuration on a release build: zero stalls,
flat 13 ms. A startup `WARN` under `cfg!(debug_assertions)` now says so out loud.

**VBV sized for latency, not streaming.** `rc_buffer_size` was one full second of bitrate,
which let keyframes reach 250–350 KB and stall the pump for 110–153 ms. It is now four frame
times.

**A playout-delay hint** to the browser, and an ICE gathering cap so a STUN server that never
answers cannot hold the offer back.

### Fixed in this release

- **Launched applications no longer outlive their session.** They inherited the daemon's own
  process group, so `child.kill()` reaped one pid and left the rest — a browser kept playing
  audio after the session that owned it was gone. Applications now get their own process
  group and a group-wide SIGTERM → 300 ms → SIGKILL.
- **The daemon shuts down on a signal.** There was no handler anywhere in the tree, so
  SIGINT/SIGTERM killed it outright and `stop_session` never ran. A calloop signalfd source
  now stops the session first.
- Session leak on relay disconnect ("a session is already active").
- `wp-fractional-scale-v1` and `wp-viewporter`, so a client told 1.5× draws at 1.5× instead
  of drawing at 2× and being composited clipped.
- Device-exact resolutions derived from the device's own aspect ratio, and a Hyprland-style
  application scale control.
- Two-finger scrolling with `AxisSource::Finger` semantics and a terminating axis-stop, so
  kinetic scrolling ends when the finger lifts.
- A console (shell + log) that floats over the picture instead of stealing height from it.

### Known limitations

- **Quality presets use a flat bitrate that ignores resolution.** `Balanced` asks for 4000
  kbps at 720p and at 1080p alike — roughly 2× starved at 1080p. Unfixed deliberately:
  webrtc-rs 0.17 has no congestion control in its send path, so an uncapped increase would
  fall apart on cellular while looking fine on a LAN.
- **The relay is not safe to deploy publicly.** The ~30-bit Remote ID is currently the only
  secret. Join rate-limiting, a confirmation gate and TLS are all required first.
- **The control plane binds localhost** because the launch command is free-form.
- Console-shell processes are still not tied to session lifetime (`server/src/exec.rs`).
- An application that calls `setsid` for itself escapes the process-group cleanup —
  observed with `chrome_crashpad`.
- No PTY: the console runs commands non-interactively, so an editor or pager will not work.

### Not included

No binaries. The daemon links the Nix dev shell's FFmpeg 8.x ABI and VA-API stack, so a
binary built here would not run elsewhere — publishing one would be worse than publishing
none. Build from source in the dev shell.
