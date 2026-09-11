# Changelog

## Unreleased

### Added

**Clients can hand over GPU buffers** — `zwp_linux_dmabuf_v1`.

`wl_shm` was the only buffer path on offer, so a GPU application had to render on the GPU,
read the result back to the CPU, write it into shared memory, and have the compositor upload
it to a texture again — two full copies of every window, every frame, on the render tick. At
a phone's 1080 × 2422 that is roughly 10 MB per surface per frame. Version 4 with feedback is
advertised when the render node is known, so a client is also told *which* GPU to allocate on;
version 3 otherwise.

**Measured, and it works.** The first session on the new build logged
`dmabuf path is live — a client is handing over GPU buffers`, with Chrome supplying `AB24`
(ARGB8888) under an AMD vendor modifier — a tiled buffer, not a linear one, so it never touches
the CPU. Each session now says which path it took, on both branches, so this stops being a
question anyone has to go looking for an answer to.

**Two-finger pinch and rotate** — `zwp_pointer_gestures_v1`.

A two-finger drag now produces a scroll axis *and* a pinch. That is what a touchpad emits and
what toolkits are written against — so the pinch's own translation is deliberately sent as
zero, or an app pans twice for one drag.

### Changed

**Logs say where things went wrong instead of going quiet.**

One `EnvFilter` sat on the subscriber registry, so it gated every destination at once — the
terminal and the client's log panel shared a single verbosity, and because that verbosity was
`info`, every `debug!` in wado's own crates was written, shipped and permanently silent. A log
line that *cannot* fire is indistinguishable from one whose subject never happened, which is
the worse of the two failures.

Filters are per-destination now. The terminal defaults to debug for wado's own crates, with
`webrtc_ice` muted — one session teardown emitted eight "Failed to close candidate" lines,
none of which ever meant anything. The client's log panel stays at `info`, because it is a
200-line ring in front of a person. `RUST_LOG` still overrides the terminal. Per-lane detail
needs no new code: a tracing target is the module path, so
`RUST_LOG=wado_compositor::headless=debug` already works.

Two questions that previously required catching a live session and reading `/proc` now answer
themselves in the log: whether any client took the dmabuf path (said on **both** branches —
"live" or "unused this session"), and which surfaces asked for a fractional scale, which is the
only way to see the population that the integer fallback actually serves.

### Fixed

**Fractional scale was implemented and then rounded away.**

The requested scale was rounded to a whole number *before* it reached either consumer, so a
session asking for 1.25 was given 1.0 on a protocol wado already speaks. The output now
carries both answers: the exact value for clients that speak `wp-fractional-scale-v1`, a whole
number for those that do not. Verified live at 1.25, 1.75 and 2.0.

The whole-number companion is floored, not rounded. Rounding and ceiling agree at 1.5, 1.75,
2.5 and 2.75, so rounding would have left the original bug — an oversized buffer overhanging
its own area and clipping app chrome — intact at every scale above 1.25. Flooring makes a
legacy client slightly soft instead, which costs almost nothing through an H.264 stream.

**A pinch that was never ended left the toolkit stuck in zoom mode.**

A disconnect or an input reset drops the gesture with no end event, and windows outlive
sessions here, so the orphan survived into the next one. The open/closed state is tracked on
the compositor side now; a new pinch closes any open one first.

### Retracted

**Jitter-buffer reclaim (shipped in v0.0.2) never worked, and is deleted.**

`jitterBufferTarget` is a floor honoured only up to what the browser's own timing model
demands — it can raise the playout delay and can never lower one the model is driving. The
reassert fired 15 times in one session while the buffer drained 56 → 55 → 54 → 54 → 53 → 52,
its natural rate, with no inflection at any of them. The reasoning is left in `webrtc.js` so
it is not tried again. The underlying behaviour is still real and still unfixed: a network
hitch permanently inflates the buffer by about 5 ms.

## v0.0.2 — `2026-09-11`

Everything v0.0.1 measured still holds; this adds the fixes found by running it. Each item
below says how far it was actually verified, because two of them have never been watched by
a human.

### Verified live

- **Launched applications no longer outlive their session.** An app spawned into the session
  inherited the daemon's own process group, so stopping the compositor killed the one pid we
  held and nothing it had forked — a browser kept running, and kept playing audio, with
  nothing left that knew how to stop it. Apps are now spawned as their own process-group
  leaders and signalled by group (SIGTERM, 300 ms grace, then SIGKILL). A signalfd handler
  runs the same teardown on SIGINT/SIGTERM to the daemon itself.
  Confirmed on the running daemon: `signal received — stopping session` → `compositor
  session stopped` → zero survivors.
- **The 1080p120 flashing is gone.** The VBV was sized in *frame times*, which made the
  allowance frame-rate dependent: at 120 fps it halved, and IDRs were being squeezed to
  ~16 KB. Sized in bits now (`VBV_MILLIS = 67`). Keyframes came back at 45–52 KB with 0
  pump stalls.

### Measured, on a release build

- **Quality presets scale with resolution.** `Balanced` and `Optimise quality` were handing
  1080p the same bitrate as 720p. Bitrate is now scaled by pixel count from a 720p base and
  clamped to 1–12 Mbps; `Custom` still passes through untouched.
- **The 1080p stutter was a debug build**, not the pipeline. Same config, release vs debug:
  stalls 14 → 0, jitter buffer 19–42 ms → 12 ms flat, frame rate 48–60 → 60 steady. The
  server now warns on startup when it is a debug build.

### Shipped, not yet confirmed by a human

- **A real shell in the console, on a PTY** (`$SHELL -l`) with xterm.js in front of it —
  resize, Ctrl-C and full-screen programs all work by construction rather than by
  observation. Server and client are deployed and byte-verified; nobody has watched it run.
  It does **not** survive a reconnect yet: the shell is owned by the relay connection, so
  losing it starts a fresh one.
- ~~**Jitter-buffer reclaim after a network blip.**~~ **Retracted** — it never worked; see
  the Unreleased section above.

### Packaging

- **`flake.nix` and `flake.lock` are in the repo**, with a real `packages.default` — so this
  tag can be `nix build`-ed. v0.0.1 predates the flake and cannot be; it is left where it is
  rather than moved, since it is already published.

### Known ceilings

- An app that calls `setsid` escapes the process-group cleanup (seen: `chrome_crashpad`).
  A per-session cgroup is the upgrade path.
- Pump stalls of 300–500 ms still appear occasionally on LAN with `rtt=0` and no packet
  loss. Duration does not track frame size or packet count, so it is a blocking event rather
  than send cost — cause not yet established, and deliberately not guessed at.

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
