# wado — Project Reference & Build Plan

> **Purpose of this file.** This is the single source of truth for the `wado` project. It is written to be read by a coding assistant (e.g. Claude Code) at the start of every working session, and by the human driving those sessions. It defines *what* we are building, *why* the key decisions were made, the *flow* of the system, and the *staged plan* to get there. Keep it open. Update it when decisions change (see "Decision Log" at the end).

---

## How to use this document (instructions for the coding assistant)

1. **Read this whole file before writing or changing code.** The architecture has tight coupling between the compositor, the encoder, and the transport; a change in one module usually has implications for the others. The "Gotchas & Hard-Won Constraints" section exists specifically to stop you from re-discovering known traps.
2. **We build and test one module at a time, in milestone order.** Do not jump ahead to hardware encoding or NAT traversal before the milestone that introduces it. Correctness first, then latency. The human will test each module manually and may change direction between sessions — always re-read the Decision Log for the latest state.
3. **Prefer the simplest path that works, then optimize.** Every milestone has an explicit "correctness-first fallback" and a "fast path." Implement the fallback first unless told otherwise.
4. **When a decision is ambiguous, surface the tradeoff and ask** rather than silently picking. Note unresolved questions in the Decision Log.
5. **Latency numbers in this doc are targets and references, not guarantees.** Network RTT dominates and is outside our control. Measure; do not assume.
6. **Licensing:** `gst-wayland-display` and Wolf are MIT-licensed and may be referenced/reused. RustDesk is AGPL — study its *design* (rendezvous, hole-punching, crypto handshake) but do not copy AGPL code into wado unless we consciously accept AGPL for the whole project. Flag any direct reuse.

---

# Part 1 — Plan & Goals

## 1.1 What wado is

`wado` is a **headless Wayland compositor written in Rust (Smithay)** that streams its rendered display over the internet with low latency and accepts remote **mouse, keyboard, and touch** input from a connecting client (notably a phone). It behaves like a remote-desktop/game-streaming host: another device connects to it from anywhere (like RustDesk), sees the display, and controls it.

It is **its own compositor** — applications run *inside* the wado session. It is not (in v1) a mirror of an already-running desktop. This "be the compositor" model is what makes the three core goals below achievable; see §1.6 for the rationale and the alternative we rejected.

## 1.2 Core functional goals

1. **Headless, remote, secure connection (RustDesk-style).** wado runs in the background without a physical display. A remote device connects to it from any network, with RustDesk-like security: a rendezvous server for discovery, NAT hole-punching with a relay fallback, an authenticated encrypted session, and password/approval gating.
2. **Per-client dynamic resolution.** Because wado controls the compositor, it spawns/sizes its output to the **connecting device's native resolution** at connect time (and can change it at runtime). A phone gets a phone-sized display. This eliminates the resolution-mismatch/letterboxing problem that RustDesk and similar tools have.
3. **Native, first-class touch.** Multi-touch is handled natively (`wl_touch`), with compositor "quirks" that make a touch-only session pleasant: on-screen/virtual keyboard, touch-to-pointer emulation for apps that expect a mouse, gesture mapping (two-finger scroll, pinch, long-press → right-click), and 1:1 touch coordinate mapping (free, because the output is sized to the device).

## 1.3 Non-negotiable principles

- **Latency target:** ~80–100 ms glass-to-glass over arbitrary internet on first trials; lower on LAN. This is achievable with hardware codecs + UDP/FEC transport, but **network RTT is the dominant, uncontrollable term.** Our controllable budget (capture→encode→packetize→depacketize→decode→display) is realistically ~25–55 ms with hardware codecs.
- **Run on any hardware, including a laptop with no reliable hardware encoder.** There must be a software-encode fallback, and **when fallback happens the end user must be told** (persistent banner in the client UI + server log).
- **Input must never be blocked by video.** Input travels on a separate, low-latency, loss-tolerant channel — never behind a reliable in-order stream that can head-of-line-block behind video frames.
- **Zero-copy on the hot path where possible**, with a host-memory copy fallback for hardware/driver combinations where DMA-BUF modifiers misbehave.

## 1.4 End-to-end architecture (the flow)

**Forward path (server → client): capture → encode → transport → decode → display**

1. **Compositor (Smithay, headless).** A custom headless output driven by a render loop. On client connect, create an `Output` sized to the client's native resolution. Composite client surfaces with the GLES2/Glow renderer into an offscreen target.
2. **Frame export (zero-copy).** Render directly into a DMA-BUF (`ExportDma` / `Bind` into a dmabuf-backed target); keep the buffer on-GPU. Provide an `ExportMem` (host-memory) path as the correctness-first fallback.
3. **Encode.** Feed the DMA-BUF to a hardware encoder (VA-API `vah264enc`, or NVENC `nvh264enc`) on the *same GPU*. Configure for zero latency: CBR, no B-frames, zero/small lookahead, on-demand keyframes (huge GOP, IDR on viewer-connect + RTCP PLI/FIR only), short GOP. Fall back to x264 `tune=zerolatency preset=veryfast` (or OpenH264) on CPU when no working hardware encoder is detected — **and notify the user.**
4. **Packetize + protect.** Handled by WebRTC: the H.264 payloader does MTU-sized RTP packetization (FU-A fragmentation of large NALs); SRTP encrypts; NACK/PLI and GCC congestion control are built in. (No hand-rolled RTP/FEC — that was the rejected Moonlight-model alternative.)
5. **Transport.** **WebRTC (webrtc-rs v0.17.1)** — DTLS-SRTP media over UDP, with ICE. **Video and input are separate channels** (input will ride a WebRTC data channel, added in M2). Signaling (SDP offer/answer) is exchanged out-of-band over HTTP — that is the control plane and may use TCP; only the media plane must avoid TCP.
6. **Client (phone).** Hardware H.264 decode (Android `MediaCodec` / iOS `VideoToolbox`), minimal jitter buffer (0–50 ms), render to screen.

**Reverse path (client → server): input**

7. Phone captures touch/mouse/keyboard → tiny packets on the low-latency channel → server synthesizes Smithay input events (`PointerHandle` / `KeyboardHandle` / `TouchHandle`) at the phone-native coordinate space. Position/motion = unreliable-sequenced (latest wins); discrete button/key events = reliable.

**Discovery & security (RustDesk model), wraps the whole thing**

8. A rendezvous server (our `hbbs` analog) for registration + UDP hole-punching; a relay fallback (our `hbbr` analog) for symmetric NAT; Ed25519-signed identity + Curve25519 key exchange + secretbox (XSalsa20-Poly1305) for the session — *or* WebRTC's DTLS-SRTP if we go that route. Password + optional connect approval.

**Why this hits the target:** HW encode ~5–15 ms + packetize/FEC ~1–3 ms + network RTT/2 ~10–40 ms (dominant, uncontrollable) + depacketize/jitter ~5–20 ms + HW decode ~5–10 ms + display ~8–16 ms ≈ 35–95 ms. Software encode pushes the encode term to ~20–40 ms — which is why the user-facing fallback notice matters.

## 1.5 Tech stack decisions (with rationale)

| Layer | Decision | Rationale | Reconsider if… |
|---|---|---|---|
| Compositor framework | **Smithay (Rust)**, start from the `smallvil` example | Matches the reference design (`gst-wayland-display` is Smithay-based); pure Rust; full control | n/a (fixed) |
| Renderer | **GLES2 via Glow**; Pixman (software) as no-GPU fallback | GLES2 is the practical, mature Smithay path. Smithay's Vulkan renderer is immature. | Vulkan renderer matures |
| Output backend | **Custom headless output**, *not* the winit backend | A Smithay maintainer states winit is "flawed… not really useful outside of prototyping" and is slated for deprecation. Use winit only for M0 learning. | n/a |
| Frame export | **DMA-BUF (`ExportDma`)** hot path; **`ExportMem`** host-memory fallback | Zero-copy keeps the buffer on-GPU into the encoder; host-memory path is the robust fallback for buggy modifiers | n/a |
| Codec | **H.264 default.** H.265 / AV1 only when the client advertises *hardware* decode | H.264 has near-universal phone hardware decode and the lowest decode latency. AV1 phone HW decode is recent (Apple A17 Pro/M3, 2023); software AV1 on a phone is a latency/battery risk. | Phone AV1 HW decode becomes ubiquitous |
| Encode (HW) | **VA-API / NVENC** via GStreamer elements *or* direct ffmpeg bindings | Lowest encode latency; same-GPU avoids copies | n/a |
| Encode (SW fallback) | **x264 `tune=zerolatency preset=veryfast`** (or OpenH264) | Proven real-time CPU encoder; zerolatency drops B-frames & lookahead to remove buffering delay. SVT-AV1/rav1e too slow for real-time on a laptop CPU. | n/a |
| Video pipeline impl | **Start with GStreamer** (reuse `gst-wayland-display`); be ready to drop it for direct encoder bindings | GStreamer is the most buildable route — the hard part (Smithay→encoder) already exists, MIT-licensed. Requires GStreamer ≥1.24 for true DMA-BUF zero-copy. | GStreamer DMA-BUF/modifier bugs cost >~2 weeks on target HW → switch to ffmpeg/VA-API/NVENC bindings directly (RustDesk's `hwcodec` crate is a ready Rust option) |
| Transport | **WebRTC (webrtc-rs v0.17.1)** — CHOSEN (`2026-06-05`). Custom UDP/RTP+FEC (Moonlight) is the rejected alternative. | WebRTC gives MTU-sized RTP packetization, DTLS-SRTP, ICE, and GCC congestion control for free; it directly fixed the raw-UDP fragmentation/large-frame corruption and lets us defer hand-rolled RTP/FEC/NAT. Slightly higher latency than the Moonlight ceiling, still under target on good networks. Pin **v0.17.x** (NOT master / the 0.20.x Sans-IO line). | Moonlight-model custom UDP/RTP+FEC only if WebRTC latency proves unacceptable on target networks (revisit at M3/M4). |
| Input transport | **Separate low-latency channel** (WebRTC data channel, QUIC datagram, or ENet/UDP) | Tiny frequent packets must not head-of-line-block behind video | n/a |
| Input injection | **Synthesize directly in the compositor** via Smithay `PointerHandle`/`KeyboardHandle`/`TouchHandle` | Lower latency than the uinput/inputtino kernel-evdev detour, and avoids permissions issues. (inputtino/uinput is the alternative the reference uses.) | n/a |
| Security/discovery | **RustDesk *model*** (rendezvous `hbbs` + relay `hbbr`, hole-punch) — design only, mind AGPL. **DTLS-SRTP (from WebRTC) supplies session crypto**, so the Ed25519+Curve25519+secretbox layer is no longer needed. | Gold standard for "connect from anywhere"; reuse the discovery/relay design. Media is DTLS-SRTP over UDP (never TCP). | n/a (WebRTC chosen) |

## 1.6 The architectural fork we already decided

There are two ways to build a remote-Wayland tool:

- **(A) Be the compositor** (Wolf / `gst-wayland-display` model). Apps run inside our session. We fully control resolution, input, and touch. **← We chose this.** It is the only model that cleanly delivers per-client resolution and native touch quirks.
- **(B) Attach to the user's running compositor** (wayvnc / RustDesk model). Uses screencopy + virtual input. But we inherit the host's fixed resolution and are limited by its protocol support — re-introducing the exact resolution-mismatch problem we want to eliminate.

**Consequence to keep in mind:** Because we chose (A), wado streams *its own* session, not the user's existing desktop. If a future requirement is "mirror the already-running desktop," that is model (B) and a separate effort (look at the modern `ext-image-copy-capture-v1` / `ext-image-capture-source-v1` protocols for that). **macOS is model (B) by necessity** — see Milestone MAC and the `2026-10-17` Decision Log entry.

---

# Part 2 — Recommendations (Staged Build Plan / Milestones)

Build in this order. Each milestone has a **goal**, **do-first (correctness)**, **then (fast path)**, and a **threshold** that must be met before moving on. The human tests each threshold manually.

## Milestone 0 — Learn the framework
- **Goal:** Understand Smithay's moving parts before building anything custom.
- **Do:** Build and run `smallvil`. Run a real client (e.g. `weston-terminal`) inside it. Read and understand: the `calloop` event loop, the centralized `State` struct, `Space`/`Window`, and `SeatHandler`.
- **Threshold:** You can render a client window and move a synthetic cursor inside the smallvil-based compositor.

## Milestone 1 — Headless output + frame readout
- **Goal:** Get rendered frames out of a headless compositor and onto a screen elsewhere, end-to-end, with no concern for speed yet.
- **Do (correctness):** Replace the winit backend with a custom **headless output**. Read frames out via **`ExportMem`** (host memory — simplest, correct). Encode with **x264 `tune=zerolatency`**. Dump to a file first (`FileSink`), then stream over **WebRTC** (`src/sink/webrtc.rs`), verified by the bundled browser test page at `http://localhost:8080`. (The original raw-UDP sink was removed — see the `2026-06-05` Decision Log entry on why it corrupted under heavy motion.)
- **Threshold:** End-to-end local video at 60 FPS with <1 s latency. (Latency is not the point yet — a working pipeline is.)

## Milestone 2 — Real transport + a phone client + input
- **Goal:** A genuinely interactive session on the LAN, including the reverse input path.
- **Do:** RTP framing + FEC are now provided by WebRTC, so this milestone narrows to: add the **separate input channel** as a **WebRTC data channel** (position/motion unreliable-sequenced, buttons/keys reliable). Build a minimal **Android client** (WebRTC + `MediaCodec` H.264 decode). Inject input by synthesizing Smithay `PointerHandle` / `KeyboardHandle` / `TouchHandle` events. Map touch coordinates 1:1 (output is already client-sized — but full per-client sizing lands in M5; here, hardcode the phone size to start).
- **Threshold:** Interactive session on the same LAN with perceived latency under ~50 ms; touch taps register at the correct coordinates.

## Milestone 3 — Zero-copy + hardware encode + fallback detection
- **Goal:** Hit LAN latency targets and behave correctly on machines with and without hardware encoders.
- **Do:** Switch the hot path to **DMA-BUF export → VA-API/NVENC** (needs GStreamer ≥1.24 if using GStreamer elements). Implement the **"try-to-open-and-cache" hardware encoder probe** (do not trust GPU presence or `vainfo` strings — actually open the encoder and cache what succeeds). Implement the **software-fallback user notification** (client banner + server log).
- **Threshold:** LAN glass-to-glass under ~30–40 ms with hardware codecs; graceful, clearly-signalled fallback on the encoder-less laptop.

## Milestone 4 — Internet: rendezvous, hole-punching, encryption
- **Goal:** Connect across two different NATs, securely, like RustDesk.
- **Do:** With WebRTC's ICE + DTLS-SRTP already carrying media (decided at M1), this milestone is: stand up a **signaling/rendezvous server** (exchange SDP + ICE candidates between peers that can't reach each other's HTTP endpoint directly) and a **TURN relay** fallback for symmetric NAT. ICE handles hole-punching; DTLS-SRTP handles session crypto (no secretbox layer needed). Add password + optional connect approval.
- **Threshold:** Phone → laptop across two different NATs; the large majority of connections use the direct hole-punched path (relay only as fallback); glass-to-glass 80–100 ms on a typical home connection.

## Milestone 5 — Touch polish + dynamic resolution
- **Goal:** Deliver the differentiators — per-client resolution and a great touch session.
- **Do:** Per-client `Output` sizing at connect; runtime resolution change. Virtual keyboard (`zwp_virtual_keyboard_v1`, which Smithay supports). Touch-to-pointer emulation for apps lacking `wl_touch`. Gesture mapping (two-finger scroll → axis, pinch → zoom, long-press → right-click). Optional cursor hiding / touch indicator in pure-touch sessions.
- **Threshold:** A phone-native-resolution session with a usable on-screen keyboard and working two-finger scroll.

## Milestone MAC — macOS host backend (designed `2026-10-17`)
- **Goal:** `wado daemon` on an Apple-silicon Mac serves the same client, relay, trust gate and
  WebRTC transport as on Linux. Only the backend behind the compositor boundary differs.
- **Shape:** a new crate `crates/macos` (`wado-macos`) implements the *same* boundary as
  `wado-compositor`: `build(frame_tx) -> (Runner, CompositorHandles)`. The server picks one by
  `cfg(target_os)`. No Apple type crosses into the server, just as no Smithay type does. Apple
  frameworks come through the `objc2-*` crates (`objc2-screen-capture-kit`,
  `objc2-video-toolbox`, `objc2-core-media`, `objc2-core-video`, `objc2-core-graphics`,
  `objc2-app-kit`). Read their current APIs at implementation time; don't recall them.
- **Threads:** AppKit's event loop owns the main thread. Sill measured that the process owning a
  `CGVirtualDisplay` must run `NSApplication.run()` (not `dispatchMain` or a bare `RunLoop`), or
  its own CoreGraphics never sees the display's modes. So on macOS `Runner::run` runs `NSApp` on
  main, tokio stays on its own threads, commands hop to the main queue, and **input is posted
  from its own thread** (`CGEventPost` is thread-safe), so it never waits behind a command or a
  frame (invariant 1). There is no render tick: the SCStream delivers frames at
  `minimumFrameInterval` on its own queue, and they are encoded there and `try_send`-ed on the
  existing drop-on-full `FrameMsg` channel.
- **Invariants on macOS:** 1 as above. 2 is unchanged (transport is shared). 3: every capture is
  an IOSurface-backed `CVPixelBuffer`; the zero-copy path hands it straight to VideoToolbox, and
  the host-memory path locks it and feeds x264. Main-display capture is the always-working
  fallback for the virtual display. 4: H.264. 5: unchanged (the same `SessionInfo` encoder
  report). 6: building the `VTCompressionSession` (and encoding one frame) *is* the probe, and
  its result is cached. 7: `RealTime`, `AllowFrameReordering=false`, a huge
  `MaxKeyFrameInterval`, `ForceKeyFrame` on demand, CBR through `DataRateLimits`. Also try
  VideoToolbox's low-latency H.264 rate-control mode (verify it exists and what it does on the
  target macOS). 8: a fresh `CGVirtualDisplay` per client at its native size, never a mutated
  one, with a stable vendor/product/serial per device so macOS remembers its arrangement.
- **Steps, each on the previous one's threshold:**
  - **MAC0 — extract the boundary (Linux-only change, testable here).** Move `CompositorCommand`,
    `CompositorHandles`, `FrameMsg`, `SessionConfig`, `Clip`, `MenuSpot` and `HitWindow` into a
    Smithay-free crate (proposed: `crates/backend`, `wado-backend`, holding types only).
    `wado_compositor::build` returns a `Runner` whose `run()` is today's
    `event_loop.run(.., after_dispatch)`. *Threshold:* Linux behaviour unchanged; tests and the
    live daemon pass.
  - **MAC1 — the server compiles on macOS.** Move `wado-compositor` and the Linux-only server
    deps to `[target.'cfg(target_os = "linux")'.dependencies]`. Gate the Linux-only server
    modules (`a11y` AT-SPI, `notify`, `host/` media/awake/phone sink, `clip` cliphist/wl-copy,
    `PR_SET_PDEATHSIG`), each answering "not on this host" the way the clipboard rail already
    does without a Wayland desktop. Add a macOS `cargo check -p wado` CI job. *Threshold:* CI
    green on macOS, and a stub backend answers `Start` with an error the client shows.
  - **MAC2 — correctness path.** SCK capture of the **main display** → lock → x264 → the existing
    channel. `CGEvent` pointer, button, scroll and keyboard (an evdev → `kVK_*` table, one file,
    one test). `Touch` is emulated as the left button at the mapped point. `ViewerAttached` and
    `ViewerVisible` start and stop the stream. A session is client-started: no SCStream before
    `Start` (Hazards). *Threshold:* a phone sees the Mac desktop, and taps land where they are
    drawn.
  - **MAC3 — per-device display.** `Start` creates a `CGVirtualDisplay` at the device's size
    (HiDPI where the device is). Any lookup failure throws and falls back to MAC2's main
    display, and the user is told. `Launch` runs `open -a`, then moves the app's new windows
    onto the virtual display through Accessibility. The window list is SCShareableContent
    windows on that display. `Stop` releases the display, and macOS moves its windows back to a
    real one. *Threshold:* a phone-native-resolution session with 1:1 taps, which still updates
    when the Mac's real screen is covered.
  - **MAC4 — fast path.** VideoToolbox H.264 straight from the `CVPixelBuffer` (zero-copy), the
    open-and-cache probe, `ForceKeyframe`, and runtime downgrade to x264. Tiers are switches on
    Display & stream, like the Linux tiers. *Threshold:* measured glass-to-glass on the LAN
    against the Linux baseline, with the number reported whatever it says.
  - **MAC5 — parity extras, one at a time.** Clipboard (`NSPasteboard` change count), the
    text-input watch (the focused element's AX role, to raise the phone keyboard), audio (SCK
    audio → the existing Opus path), window actions through AX, and congestion shedding through
    `minimumFrameInterval`.
- **Out of scope until asked:** native touch (none exists), pinch (no public gesture synthesis;
  Sill maps gestures to keyboard shortcuts), a signed `.app` and launchd service (TCC ties
  permissions to the code signature; in development the permission attaches to the terminal
  that runs the daemon), and the Intel Macs.
- **Licensing:** Sill is Apache-2.0, which can be combined with AGPL-3.0, but it is Swift, so
  this is design reuse, not code reuse. If a specific routine (the runtime-checked
  `CGVirtualDisplay` lookups, say) is ported line by line, keep its attribution and ask first.

## Decision thresholds that change the plan
- **GStreamer DMA-BUF/modifier bugs cost >~2 weeks on target hardware** → drop GStreamer; drive ffmpeg/VA-API/NVENC bindings directly (RustDesk's `hwcodec` crate is a ready Rust option).
- ~~Solo NAT-traversal/crypto work stalls → adopt webrtc-rs~~ **DONE (`2026-06-05`): webrtc-rs adopted now** (bundles ICE, DTLS-SRTP, SCTP data channels, GCC congestion control), pinned **v0.17.x** (not master/Sans-IO). The reverse threshold now applies: if WebRTC latency proves unacceptable on target networks, fall back to the Moonlight-model custom UDP/RTP+FEC.
- **Decode stutters on target phones** → stay on H.264; enable H.265/AV1 only when the client advertises *hardware* decode in the capability handshake.

---

# Part 3 — Module Reference & Working Conventions

## 3.1 Module map
The repo is a **Cargo workspace** of five crates: **`crates/compositor`** (`wado-compositor`
— the headless Smithay compositor as a **library**), **`crates/server`** (package `wado` —
the control-plane CLI binary that drives the compositor), **`crates/protocol`**
(`wado-protocol` — shared wire types), **`crates/client`** (`wado-client` — Dioxus
web/WASM app), and **`crates/relay`** (`wado-relay` — the standalone WS signaling broker:
server registry + signaling rooms keyed by **Remote ID**, a dumb verbatim forwarder after
the handshake; no GPU/Smithay deps). The server depends on the compositor and talks to it **only** through a typed
channel boundary (`wado_compositor::build` returns the event loop + state + a
`CompositorCommand` `Sender`; encoded frames come back over a `FrameMsg` channel) — no Smithay
type ever appears in the server. Module paths below are under `crates/compositor/src/` for the
compositor and `crates/server/src/` for the server, as noted.
- `protocol` (`crates/protocol`) — wire types shared by server + client so they can't
  drift: `SessionConfig`, `Quality`, endpoint path constants, and the `logfmt`
  `LEVEL|HH:MM:SS|text` format (server formats, client parses). `serde`-only, builds for
  host and wasm.
- `client` (`crates/client`) — the Dioxus **web** app (the only UI). Native form/state/log
  panel; the browser-only glue (fetch, WebRTC, `<video>.srcObject`, SSE, pagehide beacon)
  lives in `src/bridge.js`, driven via Dioxus `eval`. Talks to the server over CORS.
- `compositor` (`crates/compositor`) — the whole Smithay compositor as a library: `state`
  (the `Wado` struct), headless output + render loop (`headless.rs`), `Space`/`Window`
  management, `SeatHandler`, `handlers/`, `grabs/`, `input/`, `capture/`, `encode/`, `sink/`,
  `conf/`. `lib.rs::build` owns all event-loop wiring (Wayland sources + the panic-guarded
  `CompositorCommand` source); `control.rs` holds the `CompositorCommand` enum + handler that
  mutates `&mut Wado`. The three standalone render examples live in `crates/compositor/examples/`.
- `compositor/capture/` — frame capture behind the `CaptureTarget` trait (`mod.rs` defines it + the `Frame` currency `Rgba | Dma`), split one job per file: `mem.rs` (`MemTarget`, `ExportMem` CPU read-back — the robust fallback, invariant #3), `dmabuf.rs` (`DmaTarget`, a GBM dmabuf swapchain rendered into and exported as a Smithay `Dmabuf` — the **backend-neutral** zero-copy hand-off), `gpu.rs` (opens the GBM device + EGL-from-GBM, or surfaceless fallback). The render tick is capture-agnostic (`headless::render_tick` = bind→render→export→`submit`).
- `compositor/encode/` — encoder abstraction, one job per file: `encoder.rs` (`VideoEncoder` trait: `submit(Frame)->Result<Option<Annex-B>>` + `force_idr_next`), `select.rs` (`Tier` {VaapiDma, VaapiCpu, X264} + `tiers_for(EncoderBackend, gbm_available)` + per-tier `EncoderReport`; the concrete build lives in `headless::build_pipeline`, which tries tiers top-down — **building the tier is the probe**, invariant #6 — and `downgrade_pipeline` drops one tier on a runtime encode failure), `x264enc.rs` (software), and `ffmpeg/` (VAAPI hardware, **all ffmpeg FFI isolated here**): `mod.rs` `FfmpegVaapiEncoder` with a `Cpu`/`Dma` feed; `hwcontext.rs` (VAAPI/DRM devices, NV12 + DRM-PRIME frames); `convert.rs` (CPU RGBA→NV12, the cpu feed); `dmabuf_import.rs` (`Dmabuf`→DRM-PRIME `AVFrame`); `vpp.rs` (VAAPI VPP filtergraph map+CSC, the dma feed); `probe.rs` (standalone vaapi diagnostic, used by examples). Tiers: `vaapi-dmabuf` (zero-copy) → `vaapi-cpu` → `x264-cpu`. A future GStreamer backend (`encode/gst/`) slots in beside `ffmpeg/` behind the same trait and consumes the same `Dmabuf`. The encoder report (mode + pipeline) rides the `/session/start` JSON (`SessionInfo`) → client banner + stagebar badge.
- `server/website/` (split by concern: `mod.rs` ctx + accept loop, `routes.rs`, `offer.rs`, `pump.rs`, `sse.rs`, `http.rs`, `watchdog.rs`, `logbus.rs`) — the always-on **control plane**, **API-only** (the UI is the separate `crates/client` app): accepts session config + Start/Stop, carries the WebRTC video (`API`, shared track, frame pump, `/offer`, ICE-timeout tuning, PLI→keyframe, generation guard), answers CORS preflight (`OPTIONS`), and streams all `tracing` output to the client over SSE `GET /events` (`logbus.rs`). Holds a `wado_compositor::CommandSender` + the frame `Receiver`; the compositor owns the calloop loop. Binds `127.0.0.1` until the security gate lands.
- `error.rs` — split per crate: `wado_compositor::CompositorError` (render/encode/capture/session) and the server's `WadoError` (WebRTC/JSON/IO), which wraps the former via `#[from]`. Each has its own `Result` alias.
- `compositor/sink/` — frame sinks behind the `FrameSink` trait: `channel` (`ChannelSink`, feeds the `website` WebRTC pump — the live path) and `file` (debug/recording). Separate video vs. input channels (input data channel lands in M2).
- `compositor/input/` — synthesize Wayland input on the seat, **no on-screen cursor**, split one job per file: `remote.rs` is a thin dispatcher routing each `wado_protocol::InputEvent`; `keyboard.rs` (`Key`→`wl_keyboard` via `Keycode::new(evdev+8)`); `touch.rs` (`Touch`/`CancelTouch`→`wl_touch`, normalized 0..1 scaled to output, tap-to-focus, multi-touch per-`id`); `pointer.rs` (`PointerMotion`/`Button`/`Scroll`→cursorless `wl_pointer` motion/button/value-only-axis; button-press click-to-focus is what lights up app CSD move/resize); `window_drag.rs` (`WindowDrag`→`Wado::window_move` state machine); `common.rs` (shared input clock/coord-map/focus helpers). Fed by the server over a **separate** `calloop::channel` (`CompositorHandles.input`). App CSD move/resize is handled by `grabs/{move,resize,touch_move,touch_resize}_grab.rs` via `xdg_shell.rs` for both pointer and touch. New-window placement lives in `placement.rs`. `pinch.rs` (`Pinch`→`zwp_pointer_gestures_v1`) and client-side two-finger scroll have since landed. Deferred: kinetic scroll, touch DnD — see Decision Log + `CHALLENGES.md`.
- `security/` — rendezvous client, ICE (hole-punching via webrtc-rs), TURN relay fallback, auth. Session encryption is DTLS-SRTP (from WebRTC), not a bespoke layer.
- `signaling-server/` — SDP/ICE exchange + the `hbbs`/`hbbr` rendezvous-and-relay analogs (separate binary/deployable). The in-process HTTP signaling in `src/sink/webrtc.rs` is the M1 stand-in for local bring-up.
- `client-android/` — phone client: connect, decode (MediaCodec), render, capture & send input, show fallback banner.

## 3.2 Per-module notes for the assistant
- **Compositor:** Centralize mutable state in one `State` struct driven by `calloop`. Do **not** lean on the winit backend beyond M0. Expect to implement a custom output and render loop.
- **Capture:** Put both export paths behind a single trait so the encoder doesn't care which is active. `ExportDma` keeps buffers on-GPU; `ExportMem` is the GPU→CPU copy fallback. Always have the fallback working.
- **Encode:** Configure for *zero latency*, not quality: CBR, no B-frames, zero/small lookahead, **on-demand keyframes** (huge GOP, IDR only on viewer-connect + RTCP PLI/FIR; no periodic IDR spikes). Intra-refresh is NOT used — `h264_vaapi` and the `x264` 0.5 crate lack intra-refresh APIs (see Decision Log 2026-06-08). The HW probe must **open the encoder for real and cache the result** — presence ≠ working (NVENC/AMF/VideoToolbox open failures happen on capable hardware).
- **Transport (WebRTC):** MTU-sized RTP packetization, SRTP, NACK/PLI, and GCC congestion control are provided by webrtc-rs — do not re-implement them. Feed the encoder's per-frame Annex-B straight to `TrackLocalStaticSample::write_sample` (one access unit per call; SPS+PPS are already inline on IDR). Never carry the media plane over reliable in-order TCP (head-of-line blocking under loss is the enemy); signaling/SDP over HTTP/TCP is fine (control plane).
- **Input:** Position/motion = unreliable-sequenced (latest wins); button/key = reliable. Synthesize Smithay events directly. Coordinates map 1:1 because the output is client-sized.
- **Touch quirks:** `wl_touch` events are `down`/`motion`/`up`/`frame`; group a server frame's changes and commit on `frame`; track distinct contact `id`s for multi-touch. Emulate pointer events from single-finger taps for apps that only understand a mouse.
- **Security:** **DTLS-SRTP (WebRTC) supplies session crypto** — no bespoke Ed25519/Curve25519/secretbox layer. From the RustDesk *design* (not its AGPL code) we still reuse the discovery/relay model: rendezvous for signaling, TURN relay fallback, password + optional connect approval. Same-LAN peers can connect directly to the HTTP signaling endpoint.

## 3.3 Testing approach
- **Test each milestone's threshold before advancing.** The human runs these manually.
- **Measure latency, don't assume it.** Use the GStreamer latency tracer if on GStreamer; otherwise instrument capture→encode→send and receive→decode→display timestamps. Beware: Sunshine/Moonlight-style "latency" overlays exclude capture/encode/display, so their famous single-digit-ms LAN figures are **not** glass-to-glass.
- **Test the no-hardware-encoder path explicitly** (the laptop) at every milestone from M3 on, including that the user-facing fallback notice actually appears.
- **Test across two real NATs** for M4 (not just two machines on one LAN).

## 3.4 Gotchas & hard-won constraints (read before debugging)
- **DMA-BUF modifiers are a known minefield** across GStreamer plugins and drivers. `gst-wayland-display` shipped a host-memory copy for *years* to avoid them; true zero-copy needs **GStreamer ≥1.24**. Always keep the host-memory fallback working, and test per-GPU.
- **Smithay's winit backend is being deprecated** and its Vulkan renderer is immature — design around GLES2/Glow + a custom headless output.
- **webrtc-rs: stay on the 0.17 line.** Superseded reasoning (`2026-09-11`): 0.20 is no longer an alpha — it shipped stable, and 0.18/0.19 never existed. The pin holds for a stronger reason: 0.20's `TrackLocalStaticRTP::write_rtp` does a *blocking send on a 256-deep bounded channel per RTP packet*, a hop 0.17 does not have. Its measured send-path improvements are on the **data-channel** path, not video. Upgrading is also a full API rewrite.
- **webrtc-rs is fully async (tokio); the compositor is a synchronous `calloop` loop.** Bridge them with a bounded, drop-on-full channel (`src/sink/webrtc.rs`): the render thread `try_send`s the encoded frame and a tokio task owns the track and awaits `write_sample`. **Never block `render_tick` on the network.**
- **Browser H.264 decode needs a compatible `profile-level-id`.** Our x264 encoder uses `.baseline()`; this must negotiate as constrained-baseline in the SDP answer. If a browser shows green/garbled video, suspect a profile mismatch. Non-trickle ICE (await `gathering_complete_promise()` before answering) is sufficient on LAN/localhost.
- **Hardware encoder presence ≠ working encoder.** Probe by opening it; cache the result.
- **Network dominates latency over the internet.** On poor mobile networks you *will* exceed 80–100 ms regardless of code quality. FEC + a small adaptive jitter buffer are the only levers, and they trade latency for smoothness.
- **No authoritative single glass-to-glass number exists** for the reference stacks; vendor figures (e.g. GeForce NOW "as low as 30 ms") are best-case marketing. Treat all single-number latency claims skeptically.
- **We are the compositor, not a desktop mirror** (see §1.6). Apps run inside the wado session.
- **Wayland can't "un-advertise" an output mode.** To resize per-client cleanly, create a fresh `Output` (or reset its mode list) sized to the client, rather than trying to remove modes.

## 3.5 Reference projects
- **`gst-wayland-display`** (games-on-whales, MIT) — the closest reference design (Smithay compositor as a GStreamer plugin; supports mouse/keyboard/touch). Reuse liberally.
- **Wolf** (games-on-whales, MIT) — uses the above + the Moonlight protocol for low-latency game streaming. Study its transport, FEC, encoder config, on-demand virtual desktops.
- **Moonlight / Sunshine** — the low-latency transport model to emulate: separate channels (HTTPS pairing, RTSP setup, ENet control, RTP video), FEC, variable frame rate, full end-to-end encryption (Sunshine ≥0.22).
- **RustDesk** (AGPL — design study only) — rendezvous (`hbbs`) + relay (`hbbr`), hole-punching, Ed25519/Curve25519/secretbox crypto, the `hwcodec` crate, capability handshake for codecs.
- **wayvnc** (C) — proven screencopy + virtual-input pattern (model B); useful if we ever add desktop mirroring.
- **nvscreencopy / WayVR** (Rust, Smithay) — real-world examples of dmabuf export from Smithay and documented modifier glitches.

## 3.6 Glossary
- **Glass-to-glass / motion-to-photon latency:** time from an input action (or a pixel changing on the server) to it appearing on the client screen. The number that actually matters; usually tens of ms.
- **DMA-BUF:** a Linux buffer-sharing mechanism allowing zero-copy handoff of GPU buffers between the compositor and the encoder.
- **Modifier:** metadata describing a GPU buffer's memory layout (tiling/compression). The source of most zero-copy interop bugs.
- **FEC (Forward Error Correction):** redundant packets that let the receiver reconstruct lost packets without a retransmit/stall — essential for low-latency video over lossy networks.
- **Hole-punching:** technique for establishing a direct peer-to-peer UDP/TCP connection through NATs, coordinated by a rendezvous server.
- **Intra-refresh:** spreading keyframe (I-frame) data across many frames instead of sending periodic full IDR frames, to avoid periodic latency/bandwidth spikes.

---

# Decision Log (update this as you go)

- `2026-11-04` — **Player home / Game mode / delegated pairing: built. These are the run's own
  calls, made in an autonomous run and logged for review.**
  - **Shadowing, not writing.** Game mode applies through `Settings.overlay` in the app: shadowed
    values are never saved, and a change to a shadowed key goes to that game's profile. The web
    shapes only `SessionConfig` (`game_mode::shape`). The web's console face and arrangement are
    the computer's per-device prefs (`device { console }`), so changing them per game would write
    the computer's config. Full screen with the pad on the web is not done (TODO).
  - **Poster hue** = the app name hashed onto base08–0F, not the icon's dominant colour (which
    would mean decoding pixels during a frame). `ponytail:` marks it.
  - **Launch.** A game launches once the session is up: in the app, after a fresh `apps_list`
    or 4 s; on the web, at the first fps reading. It is skipped when the app list says the game
    is running. When a game needs the X server and a session is already running, the app asks
    "restart, or try without". A restart is a stop, a 1.5 s pause, then a start (`ponytail:`;
    the daemon's stopped event is the better trigger).
  - **Delegated codes get no `CODE_PAIRED` bonus** (join/takeover). That bonus exists because the
    device saw the computer's own screen; a delegated device saw the phone's. So it gets exactly
    `want.cap(phone)`. **Being owner does not imply `devices`.**
  - **`--for 0` is an error**, never "forever". An unreadable trust end time or code term also
    refuses (fail closed).
  - **Prune cascades through orphans.** A line whose `by` device is no longer trusted is dropped
    on the next admit, and `unpair` drops what that device minted at once.
  - **The QR card is white on the phone**, an exception to the Sixteen Slots Rule as Stage
    Black is, because a reader needs dark on light.
  - **Deferred to TODO:**
    - a TUI Devices column for `devices` (`wado allow … devices` works today);
    - a QR on the web Devices page (the web has no encoder; it shows the typed code and link);
    - the web poster-grows launch moment;
    - suggestions when a game is played with the pad;
    - compositor Game-mode rules.
  - **Rebuild pattern:** `Settings.keys([...])` is the way a widget listens to settings. A
    `ListenableBuilder` on all of `Settings.I` is what made home rebuild on every slider tick.

- `2026-11-04` — **Player-first client, many computers, delegated pairing: direction taken, not
  built. Decided with the user (multiple choice).** Spec:
  `docs/superpowers/specs/2026-11-04-player-home-and-devices-design.md`.
  - Home is library-first and adaptive. Game mode is a per-app switch set up once. Full screen
    holds the pad. Hints are just-in-time, explain-locked, a ? overlay, and a short tour.
  - Many computers: a dropdown, an all-computers home of widgets, pull-only handoff, and
    "Leave running" as the pause. Clipboard on home is shown in full, a risk the user accepted.
    A computer added by scan starts with clipboard sharing off.
  - `wado pair` asks for grants and expiry when run bare. Trust entries gain an end time that
    the computer enforces.
  - **This partly reverses `2026-10-30`'s "no remote approval".** A phone holding the new
    *Manage devices* grant may ask the computer to mint an ordinary pairing code. There is no
    reverse-QR approval. Guardrails:
    - only the computer can give the grant;
    - a passkey is needed for each mint;
    - a delegate gets no more than the phone has, and never *Manage devices*;
    - a delegate lasts at most 7 days;
    - signing out a phone signs out everything it minted for;
    - a phone may sign out only devices it minted for;
    - the computer logs and announces every mint.
  - CLAUDE.md's device-trust hazard stays as it is until phase 3 lands.

- `2026-10-31` — **The app's default session shape fits the console, not the screen. Asked for by
  the user** ("select an aspect ratio that fits horizontally completely in the intermediate page").
  - With Shape left on its default, the Flutter app (`app/`) sizes a new session to the console's
    picture room (`console_layout.dart` `pictureRoom`), the way round the session will be: the
    picture fills the console between its rails. Short side 720, as before.
  - Full screen then letterboxes a little instead of the console doing it. The screen's own sizes
    stay one tap away on the Shape page ("fits full screen").
  - Invariant #8 is unchanged: a different shape is still a fresh output (reconfigure or start).

- `2026-10-31` — **Native Android P1 merged** (plan A9 decisions executed).
  - `crates/client-core` is pure Rust, shared with desktop; `mobile-core` is only the UniFFI layer.
  - The core uses `ring` 0.17, as the daemon does.
  - **Pairing:** only on the seat join, then the phone leaves with close code 4001. Every feature
    uses control rooms, and a relay without `control` + `room_strict` is never joined.
  - The app pairs as a new device; home shows "Pair the app" on computers only the web part knows.
  - Navigation 3 (1.2.0) with a saved back stack and predictive back.
  - Size is local-first; it is not yet mirrored to computers (P2).
  - **OS logos:** Simple Icons 16.36.0, **CC0 only**. NixOS, Debian, Fedora, Gentoo, Rocky and MX
    fall back to the parent distro's logo or Tux. Allowing CC-BY would bring NixOS back; that's
    the user's call.
  - Core events are pull-based; the core never calls into the JVM.
  - **Client keys are UUIDs:** the relay keeps only `[A-Za-z0-9-]` of a client key, so base64url
    keys broke pairing about half the time.
  - The native dim text colour never drops below 3:1 contrast. The web client's dim on light
    schemes is 2.4–2.8:1 and still to fix.

- `2026-10-31` — **Live events over wado's own connection, per computer. Asked for by the user.**
  - Each daemon sends typed, sequenced, sealed events (notify, playback with position plus
    `at_ms` and rate for extrapolated live sync, volume, presence), with replay from a seq ring
    on reconnect.
  - In the background the app holds one long-lived link. Several computers are multiplexed
    through a new relay *device inbox* (additive cap `inbox`; the relay forwards sealed events
    unread). The relay itself reports `host_gone`.
  - The phone shows notification channels and one MediaSession per computer, each labelled.
  - No FCM. Native plan addendum; P1 readies the link, P7 builds it.

- `2026-10-31` — **Push without third parties is the default in the native app. Asked for by the
  user** ("push notifications directly without any other components").
  - The computer can't reach the phone (carrier NAT), and Android closes idle background sockets.
  - When no UnifiedPush distributor is installed, the app keeps its own link to wado's relay: a
    foreground service of type `remoteMessaging` with a silent ongoing notification, and
    keepalives every few minutes rather than 15 s. Push notes from the daemon reach it there.
  - FCM is not used. UnifiedPush stays as the lighter option.
  - The relay also says "<computer> stopped answering" when a daemon drops, which the computer
    itself can't.
  - Goes into native plan P7; the "No UnifiedPush app" error card goes away.

- `2026-10-31` — **Relay-link hang fixed (merged).**
  - **Cause:** the lane writer stopped reading its channel while stuck in a socket write. The
    128-slot `out_tx` then filled, the main loop's `send(Pong).await` parked forever, and
    `RELAY_SILENCE` could never fire.
  - **Fix:**
    - The writer always reads, even while waiting for budget or writing.
    - Bulk past 16 MiB is dropped and logged.
    - A socket write stuck for 45 s ends the link.
    - A watchdog reconnects a loop that has read nothing for 50 s.
    - Clipboard copies over 1 MiB stay off the lane (`ClipHeld` note, additive). Clipboard has no
      direct path yet, so such pictures stay on the computer.
  - Regression test: `lane::pump::tests::a_stuck_socket_never_stalls_the_reader`.

- `2026-10-30` — **Native Android plan taken** (`docs/superpowers/plans/2026-10-30-native-android.md`;
  its 13 decisions are listed there).
  - **Media:** libwebrtc via `io.getstream:stream-webrtc-android` 1.3.10, pinned (Apache-2.0 over
    BSD-3), with SurfaceView hardware decode. Every SDP/ICE message goes sealed through the core.
    Input data channels send from a dedicated thread.
  - **Terminal:** Termux `terminal-emulator` / `terminal-view`, vendored (Apache-2.0).
  - **Core:** a new pure-Rust `crates/client-core`, shared with the desktop app;
    `crates/mobile-core` becomes the thin UniFFI layer.
  - **Links:** one seatless control room per computer, plus a view room to join a session. No
    feature uses the seat. Pairing alone uses the seat join, for the ceremony only.
  - **The app is a new device:** re-pair once per computer. The phone shows twice until P8.
  - **Text and interface size are phone-local in the app**, mirrored best-effort to computers.
    This supersedes "stored on the computer" for the native app.
  - **Navigation 3** for back and predictive back (fallback: Navigation-Compose 2.9).
  - **Touchpad variants:** edge and wheel scrolling emulate a two-finger scroll; Boost changes
    speed via Hyprland; the click bar sends real buttons.
  - **Budgets:** cold start ≤ 450 ms p50; 120 Hz scroll with frames p95 ≤ 6 ms; input to send
    ≤ 2 ms p90; glass-to-glass ≤ the WebView's; APK ≤ 25 MB after P8.

- `2026-10-31` — **One UI framework for every platform: Flutter (Dart). Decided by the user**
  (after comparing Compose Multiplatform, Tauri 2, Dioxus native, Slint, React Native and one native
  UI per platform). Covers Android, iOS, iPadOS, macOS and Linux. **Supersedes the framework part of
  both "Jetpack Compose" entries below**; their product decisions (no WebView in the app, one native
  home then per-computer pages, protocol in the Rust core, hardware video decode, MediaSession,
  push over wado's link) still hold.
  - **Why:** no JVM on any platform (AOT-compiled everywhere; the user rejected Compose
    Multiplatform for its JVM desktop); one UI codebase; Android libraries called directly from
    Dart through `jni`/jnigen 1.0 (no method channels), which keeps the door open for the computer
    driving phone features, mobile controls and custom actions; one WebRTC library on all five.
  - **Ruled out:** Tauri (a WebView app again; WebKitGTK has no working WebRTC on Linux, the CEF
    runtime is alpha); Dioxus native and Slint (mobile early, video client to build ourselves);
    React Native (no WebRTC on macOS, no official Linux); Compose Multiplatform (JVM desktop).
  - **Libraries:** `flutter_rust_bridge` to `crates/client-core`; `flutter_webrtc` for video and
    data channels; jnigen (Android) and ffigen/swiftgen (Apple) for platform APIs.
  - **Cost accepted:** the P1 Compose screens are redone in Flutter; Android-only Compose widgets
    can only be embedded as platform views.
  - F8's Slint host app is unaffected. The web client stays for browsers.

- `2026-10-30` — **(Framework superseded 2026-10-31: Flutter.) The native Android app is Jetpack Compose. Decided by the user** (chosen over
  Compose Multiplatform, Flutter and Slint).
  - Every page is Compose; the WebView, its bridge and the bundled web client are removed from
    the app.
  - The protocol lives in the Rust core over UniFFI.
  - Video: libwebrtc for Android, with SurfaceView and MediaCodec hardware decode.
  - The middle web home goes away: one native home, then per-computer pages.
  - The web client stays for browsers and iOS.

- `2026-10-30` — **Parallel use by default; take over only per exclusive feature. Decided by the
  user.** Every validly approved device can use every feature at the same time. No device has to
  take over, or hold the seat, to use a basic feature or to start a session.
  - **Session:** Desktop on a running session **joins it with both controlling** (F6 arbiter).
    Start my own is one tap. Take over is only for resizing the session to your screen.
  - **Host Touchpad and Keyboard:** **one holder each**, separately. A second device sees "In use
    by <device> — Take over?" on the door, the quick tile and the hub.
  - **Gamepad:** **each device gets its own virtual pad** (player 1/2/…), never exclusive.
  - **Take over between equals** (same grants) is **instant**, with a toast and a one-tap Take back.
  - Sharing (`docs/superpowers/specs/parallel-use.md` to be written):
    | Feature | Mode |
    |---|---|
    | Files, clipboard (and history), shells (one per device), Claude, notifications/push, widgets and quick tiles (volume, brightness, media, Wi-Fi, BT, keep awake), sessions list, settings | shared, concurrent |
    | Session picture and input | shared (join + co-control); the lead's size is taken over |
    | Host touchpad | exclusive, take over |
    | Host keyboard and typing | exclusive, take over (separate from touchpad) |
    | Host actions (rail, quick actions) | shared (one-shot commands) |
    | Gamepad | per device |
    | Pointer lock in a session | the lead only (as F6) |
  - Mechanism: every device gets its own link (F6's seatless `control` rooms, now on the relay);
    the seat stops gating features. Holds live per feature in the daemon (`holds/`), announced
    to all devices.

- `2026-10-30` — **Session trap fixed; the session page reworked (merged).**
  - **Root causes:**
    - `native::go` ignored every door while a session was on.
    - Android Back toggled the control centre.
    - Late `session_started` answers re-entered the session.
  - **Rule "left means left"** (`js/session_intent.js`): only an explicit pick re-enters, and a late
    start is answered with `session_detach`.
  - Entering pushes a history entry, so Back leaves.
  - Leave lands on the Computer page and stops this device's retries; the session keeps running.
  - Resume restores home or the Computer page; a crash-rejoin only if a picture was showing.
  - The connecting and no-picture states live in the stage, with Leave and Retry.
  - In the console, the IME keeps the stage height and opens only on Type or ⌨.
  - Transparent video poster, in the page and in the app.
  - Additive acks on session, host, config and clipboard requests, plus launch outcomes, are on
    the wire; the pending primitive is built but not yet wired into actions.

- `2026-10-30` — **F11 Virtual input merged.**
  - **Saved gamepad layouts:** `gamepad { mine { <id> "<name>" "<base>" "<json diff>" } }`, set
    through ConfigSet `device.<k>.gamepad.mine.<id>`. At most 8, each at most 4 KB.
  - **Built-in layouts are never edited:** the first edit forks one into `mine-<base>`. Shown
    names carry no brand words.
  - **The gamepad is held 30 s after GameOff, Bye or a lost link:** everything is released at
    once and the device is removed after 30 s. Player number is its order among wado's live pads.
  - **Pointer speed is the computer's:** the phone's real size is declared, and speed, scroll,
    natural scrolling, tap, tap-then-drag and drag lock are applied runtime-only to Hyprland's
    `device[wado-trackpad]` (cap `pad-hypr`).
  - The in-session overlay is drawn at 0.62 × the layout's opacity. The console deck packs groups
    by side, and a group that can't fit at 44 px is left out.
  - The action-run seam for the feedback work is `ui/computer/input/open.rs::run_action`.

- `2026-10-30` — **Entering a session is always the user's choice, and leaving always works.
  Decided by the user** after being trapped in a failing session ("clicking anything returns to
  the session, from where I can't get out").
  - Navigation is a stack: home → Computer page → pages → session.
  - The session is entered only on the user's pick. Retries, rejoins, link recoveries and app
    resume never pull them into it.
  - Back from the session always reaches the Computer page, and stops this device's retry loop
    without ending the session on the computer.
  - Resume restores the page the user was on, not the session, unless it was on screen and
    showing a picture.

- `2026-10-30` — **The Android app goes fully native; the WebView client is removed from it.
  Decided by the user:** "Remove the web client part and implement natively — that's why it was
  feeling so off and unmatched from the home screen experience."
  - Supersedes F9's "bundled client in a WebView as the fallback for everything not yet native"
    for the app.
  - The web client stays for browsers and iOS.
  - **Shape:**
    - The client protocol moves into the shared Rust core (`crates/mobile-core`, to become
      the client core also usable by the F8 desktop app): relay link, E2E handshake and sealing,
      pairing, and the message codecs for session, files, shell, pad, clipboard, passkey, push and
      Claude. One implementation, tested against the JS client's vectors.
    - Compose draws every screen.
    - Media uses a native WebRTC stack with MediaCodec hardware decode. The stack is chosen in the
      plan and must keep invariants 1, 2 and 4.
  - **Phased:** each phase lands native screens and the WebView is deleted last, so the app keeps
    working throughout.
  - **Also asked for:**
    - a single native home with per-computer pages
    - a distro or OS logo per computer, from an open-source icon set
    - more natural quick controls per device
    - "On this phone" settings in the native home
  - Plan first: `docs/superpowers/plans/2026-10-30-native-android.md`.

- `2026-10-30` — **Every action shows its state. Asked for by the user.** There is no frozen button
  anywhere. One pending-action primitive drives four stages: pressed under 100 ms, working by
  ~300 ms with in-place progress, a plain "still waiting for …" past the action's threshold with
  Cancel and Try again, and the outcome (quiet success; a plain reason and Try again on failure or
  no answer). It is resolved by the action's answer on the wire; actions without one get an
  additive acknowledgement. The console's unresponsiveness is measured with a CPU-throttled trace
  before any fix.

- `2026-10-30` — **A passkey must never trap the person. Asked for by the user:** "should not
  restrict user in this way … other people might get into different problems".
  - Every ceremony stage has a timeout and an exit: Try again, Not now, plus two escape hatches.
  - **Approve on the computer:** a request accepted by someone at the computer
    (`wado passkey approve`, the TUI, a desktop notification) opens the same unlock window. It
    expires after 2 minutes and is rate-limited.
  - **The owner can turn the passkey requirement off per device:** default on. Set from the
    computer, or from the phone while unlocked, with an honest warning.
  - Evidence that started it: three unlock challenges sent from the live daemon were never
    answered or refused; the authenticator step on the phone hung with no timeout.
  - Being built by an agent; the touchpad feel research runs in parallel and feeds F11.

- `2026-10-30` — **Layering fixed; file viewer rebuilt (merged).** Three root causes:
  - The console's `.cground` rule hid the Computer page's own `.cground` (renamed `.sground`,
    `.cplate.pic`).
  - `--stage` was never defined, so the viewer was transparent.
  - Covered pages stayed painted and Dioxus `inert` was misused.
  
  **Rules:**
  - A covered page becomes `visibility: hidden` once its exit motion ends and is inert from the
    start.
  - Use `ui::widgets::inert()` for the attribute.
  - Namespace console classes.
  - `scripts/layers-check.mjs` (paint, text overlap, hit-testing, mid-transition) went from
    98 failures to 0.
  
  **Viewer:**
  - `js/viewer/` has one module per kind (image, video, audio, pdf, text/code, md, json, csv,
    zip listing via the new `FileReq::Entries`, font, info card).
  - Every file opens in it; unknown files get an info card.
  - Details is a pull-up sheet on a phone and a side panel from 900 px wide.
  - It is not scaled by the interface-size zoom (pan-zoom maths).
  - The photo filmstrip is removed.
  - On the relay lane, files over 1 MB and all media wait for a direct connection.

- `2026-10-30` — **Size built (merged).**
  - **Text size:** every font is in rem from one root, `--text-base × --text-scale`, from 85 to
    150 %.
  - **Interface size:** `--ui-scale` applied as CSS `zoom` on pages and sheets only (Compact 0.9,
    Cozy 1, Comfy 1.1, Large 1.2). The stage, console, trackpad and gamepad are excluded.
    - Zoom rather than tokens: about 1000 px values across files being edited in parallel.
      Revisit if `zoom` misbehaves on Safari.
    - Text grows with interface size, as Android does.
  - Stored as `device "<key>" { size { text follow interface } }` and self-settable. Cozy at
    100 % is the default.
  - **"Follow phone" is off by default:** the app ignores a large system font until the person
    opts in.
  - The Android WebView pins `textZoom = 100`; the phone's font scale arrives as the `fontScale`
    message.
  - `scripts/responsive-check.mjs`: 251 of 600 states failed before; 0 of 1800 after.

- `2026-10-30` — **F4 reliability merged.**
  - **Reconnect is measured** (`scripts/reconnect-time.mjs`): a network change to the first frame
    went from a median of 4254 ms to 862 ms. Both ends now send their SDP once a server-reflexive
    candidate is in (or 700 ms after the first host candidate) and trickle the rest. No ICE
    restart: the 2 s target is met.
  - **Lane.** The relay enforces 64 KB/s per seat each way (cap `lane`) with a **2 MiB burst**,
    sized for the sealed apps list and F6's speed test. One budget per seat survives reconnects.
    The daemon also paces itself, so the cap holds against older relays.
    - Priority: signalling and the speed test go first. Shells, files, clipboard history and
      host state wait for room. Log lines are sent only when the link is idle.
    - Files on the lane are capped at 1 MB. Queued frames for a departed device are dropped.
  - **Badge.** It reads Online / Streaming (direct) / Relayed — limited. Hints appear at the three
    places the cap hurts, with Retry direct.
  - **Relay health.** `scripts/relay-health.sh` plus `deploy/relay/uptime.md` for an outside
    check. A push after 90 s without the relay follows the existing "computer online" switch.
    `wado relay status [--json]` and the TUI show it.
  - **Compact touch frames:** 59 B to 13 B, negotiated through ping/pong `bin`, with JSON as the
    fallback.
  - **Idle seat (cap `idle_seat`).** A seat held with nothing running says "Your other device …
    is connected — nothing is running", with one-tap Use it here; Sessions is never 0 next to
    "in use". Starting without moving the seat is not done: rooms only join running sessions.

- `2026-10-30` — **Passkey: the dead button, and how long an unlock lasts (merged).**
  - **The bug.** The passkey sheet was `inert` while open. Dioxus writes `inert="false"`, and any
    `inert` attribute makes an element inert, so every tap on the sheet fell through to the
    scrim and cancelled. That broke the post-pairing sheet and every locked door, in the browser
    and the Android app; the Claude picker had the same bug.
  - **Rule: never write `inert: bool` in Dioxus; use `.then_some("true")`.** Real-browser tests use
    real mouse events (`scripts/passkey-ui-e2e.mjs`), because a programmatic `.click()` ignores
    `inert`.
  - **No silent drops.** The daemon answers every passkey request with a state and a plain error,
    and joined rooms route passkey messages. The client's busy state gives up after 20 s.
  - **Lifetime.** Stored in `passkeys/<device>.stay`, not `config.kdl`: unpairing deletes both
    together, and a change needs an unlock.
    - A non-default choice survives a reconnect when the trust line has a proven key.
    - "Until I lock it" also survives a daemon restart, on a code-pinned line only.
    - Timed choices re-ask after a restart.
  - **Locking from the computer:** `wado passkey lock <device|all>` (tells every running daemon
    over its socket), and `L` in `wado tui`.
  - **Trackpad refusals** are actionable cards (`js/pad_why.js`).

- `2026-10-30` — **Virtual input design taken (`docs/superpowers/specs/2026-10-30-virtual-input-design.md`).**
  - Gamepad uses the `input` grant + passkey step-up; no new grant.
  - One layout model and one renderer for sessionless play, the in-session overlay and the console
    deck.
  - Positions are anchored to the screen edges of a reference phone; sizes are in CSS px.
  - Face buttons bind by position (Xbox codes).
  - Variant names carry no brand words.
  - New pad messages `GameOn/GameOff/Btn` (reliable) and `Axes` (unreliable, resent every 100 ms,
    zeroed after 250 ms of silence), gated by a `pad-gamepad` cap.
  - `trackpad { enabled }` keeps its name and gates all three modules.
  - The uinput pad is kept for 30 s after leaving the screen.
  - Sessionless Keys & mouse is deferred: `KeyInjector` can't hold keys, and there is no
    relative-mouse device.

- `2026-10-30` — **Text size and interface size are device settings; cozy is the default. Asked
  for by the user.** Appearance gets fine Text size (85–150 %, plus "Follow phone") and Interface
  size (density) controls, stored per device on the computer and self-settable. Both are applied
  through `:root` scale tokens that the type ramp and spacing derive from, never by blind zoom,
  and touch targets stay at least 44 px. The Android WebView pins `textZoom = 100` and passes the
  phone's font scale to the page as a hint, because inheriting it overflowed a layout designed at
  100 %.

- `2026-10-30` — **Pocket Console built (F5, merged).** Sessions open as a console by default;
  Full screen morphs to the cockpit and asks for browser fullscreen. Scale, never resize: the
  console is a transform on the video. **The deck's faces drive the session's input channel, not
  the host's virtual touchpad** (`js/deck_pad.js`, relative pointer + a drawn dot). Quick face =
  session workspaces + the computer's player; "one list, three doors" not met — revisit in F11
  Virtual input. Device prefs gain `device { console { open face } }` (`face` defaults to `auto`),
  self-settable like `trackpad`. `fullscreen` host action displays as "Window full screen".
  Build notes: spec §13. Not measured: decode-drop/fps console vs cockpit on a phone.

- `2026-10-30` — **Virtual input replaces the separate Trackpad door. Asked for by the user.**
  It has three subcategories: Touchpad (mouse), Keyboard and Gamepad. All three work without a
  session, and their settings live in this category. Quick menu and widgets keep one-tap shortcuts.
  The gamepad is being revamped:
  - layout variants (including the session's own layout) and saved custom layouts
  - an editor for position, size, look and shape
  - an animated build; the same layout model in and out of a session
  - outside a session it drives the host's uinput pad through `GamepadInjector`
  Sequenced: a design pass (spec, prototype, shots) now; the build after F5 and F4 merge, because
  F5's console deck mounts the same gamepad and touchpad pieces.

- `2026-10-30` — **Passkey unlock lifetime is the device's choice. Asked for by the user:**
  "allow a device to once authenticate and then it can choose if they want to put expiration
  based passkey as optional". This supersedes the fixed 15-minute window as the *only* behaviour,
  which stays the default. While unlocked, a device can pick "Stay unlocked": 15 min idle, 1 hour,
  8 hours, or until it locks itself, plus "Lock when I leave the app". The daemon refuses a change
  while locked. The choice is stored on the computer per device, and a non-default unlock survives
  a reconnect (and, for "until I lock it", a daemon restart). The off-ramps are Lock now on the
  device, `wado passkey lock` and the TUI on the computer, and `wado passkey remove`.
  Also fixing: the "Add a passkey" button did nothing on a real phone, and the daemon drops passkey
  messages silently on some paths.

- `2026-10-30` — **F10 clipboard revamp merged (autonomous run).**
  - Every copy passes one hub (`server/src/clip/hub.rs`): hashed, classed new / echo / secret.
    **A secret never crosses** between desktop, session and devices; password-manager hint types
    mark it (`wado_platform::clip::is_secret`), and wado writes secrets with `wl-copy --sensitive`
    or not at all.
  - Automatic two-way sharing per computer, switched on the phone; on by default only for a
    code-paired device (`DeviceGrants.paired`) with the `clipboard` grant. Every message re-checks
    the grant. A device never gets its own copy back. Safari-style refusals fall back to a one-tap
    chip; sensitive phone copies only go on a deliberate tap.
  - History without cliphist: last 50 in memory, 96 MiB cap, pins. Caps: text 1 MiB, PNG/JPEG
    8 MiB.
  - One Clipboard component (Computer page section, compact as a home widget); the landing-page
    rail is gone.
  - Known: in a pool a peer's push is labelled "desktop"; the secret list is in memory only, so an
    old cliphist without sensitive support can re-show a secret after a restart until re-copied.
  - A joined room (F6) sets only the session clipboard, never the host's — consistent with F6.

- `2026-10-30` — **F6 multi-client merged (autonomous run; implementation notes at the end of
  `docs/superpowers/specs/2026-10-30-multi-client-design.md`).** Calls the agent made, accepted:
  - No `E2eHello` version bump (it is in the handshake transcript). A joinable daemon is seen per
    session by `max_participants > 0`; a capable relay by its `control` + `room_strict` caps.
  - A joined room has session input and verbs only — no shells, files, trackpad or host clipboard
    history over it; those use the device's own link.
  - **Session input has no passkey step-up**, for the lead or joiners, matching the seat today
    (only trackpad, shells and file writes are gated). Hook point if wanted: `Hub::join`/`takeover`.
  - Ending a session you are not in needs the `takeover` grant.
  - The speed test measures the relay path, not the media path; labelled as an approximation.
  - Old relay/daemon: Join hidden with the reason; Take over and End fall back to seat takeover.
  - New grants `join`, `takeover`: given by any pairing code; older trust lines need `wado allow`.
  - New config `server { max-sessions 4, max-participants 4 }`; sibling UDP ports follow instance.
  - Open: T11 measurements (encode headroom, lead g2g with 1–3 viewers, join keyframe cost).

- `2026-10-30` — **F9 Android app merged (autonomous run; `android/`, `crates/mobile-core`,
  `docs/android.md`).** Native Kotlin + Compose shell with the shared Rust core over UniFFI.
  - The web client is bundled in the APK at `https://appassets.androidplatform.net/` (no remote
    code, no service worker) and is the fallback for everything not yet native. One WebView per
    process; the `window.wadoNative` bridge is injected only for that origin.
  - Native today: home cards, trackpad (pacer ported to `mobile-core`), QR pairing, share sheet,
    settings, haptics, clipboard both ways (skips sensitive clips), lock-screen media + volume keys,
    notifications with Claude Allow/Deny, UnifiedPush (one endpoint per computer).
  - **Passkeys in the app are the app's own:** Android's providers refuse the asset origin, so the
    app answers WebAuthn itself with a Keystore P-256 key gated by the screen lock on every use.
    Per device, not synced, separate from the browser's passkey on the same phone.
  - Not yet native: the video session (libwebrtc + MediaCodec), the Computer page, the gesture
    rail. Queued as F9b.
  - The flake's two Android shells accept the Android SDK licence (`android_sdk.accept_license`);
    nothing outside those shells does. The user should confirm they accept it.
  - Package `io.github.sandptel.wado`, minSdk 26, target 37. Release signing is the user's key
    (`docs/android-release.md`); none is generated.

- `2026-10-30` — **F5 Pocket Console design taken (autonomous run;
  `docs/superpowers/specs/2026-10-30-pocket-console-design.md`, prototype and shots beside it).**
  - The console is the default arrangement on every form factor; Full screen is one toggle that
    morphs to today's cockpit and also requests browser fullscreen.
  - **Scale, never resize.** The console transforms the picture; the session's output never changes
    size for it, so invariant 8 and touch mapping are untouched.
  - **Not in v1:** sizing the session to the console. It would need a fresh Output and an IDR on
    every toggle, or a letterboxed cockpit. Revisit once text legibility at ~69% is measured.
  - The `Fullscreen` *action* is relabelled "Window full screen" (same id) so it does not collide
    with the console's Full screen toggle.
  - One new duration token pair, `--dur-stage` / `--dur-stage-out`; no new curve.
  - Built after F6 merges: the people chip and pointer dots share F6's participant state.

- `2026-10-30` — **F3 merged: passkeys, Claude from the couch, Web Push.** The agent made these
  decisions, accepted by the orchestrator in autonomous mode:
  - **Passkey:** the unlock is bound to the relay link, so a new connection starts locked. UV is
    required. A passkey is bound to its origin and RP id. Replacing one needs the old passkey or
    `wado passkey remove`.
  - **What the passkey gates:** Allow needs the passkey; Deny does not. The folder listing for a
    new Claude session is passkey-gated too.
  - **Claude:** a permission request waits up to 300 s, or answers "none" at once when nobody can
    answer.
  - **Push:** one push computer per browser (a browser has one subscription per worker scope).
    "Computer offline" is not sent; only "back online". The SSRF guard allows private LAN
    addresses. The VAPID contact is a placeholder.
  - **Also:** `wado_platform::pty` gained `Program::Command { argv, env }`. Push is
    endpoint-agnostic (`PushSub.kind` web|unifiedpush), with a `PushNote` schema in
    `wado_protocol::push`.
  - **Open on a phone:** a passkey on Android and iPhone, a push on a locked phone, and how
    Claude's terminal prompt behaves while the hook waits.

- `2026-10-30` — **F8 desktop app v1 on `experimental/desktop-app` (Slint 1.18.1, taken under
  GPL-3.0-only, compatible with AGPL via §13 of each).**
  - **Shape:** `crates/desktop` (`wado-desktop`), not a default member. One app with a host console
    (overview, devices and grants, pairing with QR + typed code + check words, sessions, logs, all
    config sections with staged validation) and a client (computer cards, per-computer page).
  - **Reuse:** it reuses gate, pairing and config write-back from the daemon crates.
  - **Not yet:** native desktop streaming is planned only (webrtc-rs + VA-API decode into a Slint
    image); for now Desktop/Trackpad/Files/Shell open the browser client.
  - **Follow-ups:** switch onto `wado-platform` watches; native streaming.

- `2026-10-30` — **Platform layer merged (steps 1–5, step 6 partial).**
  - **Where it lives:** `wado-platform` (`crates/platform`) *is* MAC0's "wado-backend" crate,
    renamed and merged. Linux providers live in `server/src/platform/linux`.
  - **Host state:** published on watch channels and read with per-capability ages: audio, media
    and brightness 1 s; Wi-Fi and Bluetooth 5 s; an action re-reads its own capability at once.
  - **Measured:**
    - `slot_events`: 340 → 32 ns per frame, with no allocations.
    - uinput writes batched: 10 → 1 syscall per touch frame, 8 → 1 per key tap.
    - Host state poll: 12 processes / 137 ms → about 3 processes.
  - **Behaviour fix:** "This phone" output can now be played on and made default (it was hidden).
  - **Not yet done:**
    - Video sits behind `VideoBackend` but not the service wrapper, because the GL loop stays on
      main.
    - No "run inside an existing loop" variant.
    - `CapSet` is not yet sent to clients.
    - Desktop display capture is still in the server.

- `2026-10-30` — **(Framework superseded 2026-10-31: Flutter.) Android client framework: native Kotlin + Jetpack Compose, with a shared Rust
  core via UniFFI (supersedes the WebView-only plan above). Decided by the orchestrator per the
  user's brief: "extreme performance, great Android support, creative design freedom".**
  - **Video:** libwebrtc Android SDK with hardware MediaCodec decode.
  - **Shared Rust core:** pad pacing, the e2e envelope and the check words.
  - **Screen order:** native screens first (home, Computer page, desktop session, trackpad,
    pairing). The bundled-client WebView remains the fallback for long-tail screens until they are
    ported.
  - **Push:** UnifiedPush, reusing F3's RFC 8291/VAPID sender, which is now endpoint-agnostic.
  - **Native extras:** haptics everywhere, and a MediaSession mirroring the computer's now-playing
    (lock screen and Bluetooth). Phone volume keys drive the computer's volume, and changes flow
    back. Laptop audio plays on the phone via "This phone".
  - **F10 clipboard revamp (web + Android + daemon):** automatic both-way sharing. Host copy goes
    to the phone; phone copy goes to the host on focus. Sensitive and password-manager copies are
    never synced. History, share target, and full accessibility labels. Scheduled after the
    platform migration's clipboard step, to avoid moving it twice.

- `2026-10-30` — **F9 Android app (user asked for an APK; plan by the orchestrator under
  autonomous mode; `.superpowers/sdd/run-after-trackpad/F9-android-brief.md`).**
  - **Shape:** a native Kotlin shell hosting the same Dioxus client in Android System WebView
    (Chromium, so WebRTC works). The client is bundled in the APK and served via
    WebViewAssetLoader, with no remote code loading.
  - **Native adds:** a true orientation lock for the landscape trackpad, haptics, WebAuthn in the
    WebView, a share target ("Send to computer"), app shortcuts, QR scan in JS, and native
    notifications while connected.
  - **Background:** an optional "Stay connected" foreground service on the seatless control link.
    There is no FCM; the Chrome-installed PWA keeps true Web Push.
  - **Build:** a Nix Android shell, which **requires `android_sdk.accept_license = true`**
    (flagged to the user). The APK is debug-signed for now; release signing uses a keystore the
    user holds.
- `2026-10-30` — **Relay redeploy to Azure blocked by the harness (production deploy) → user step.**
  The stripped binary is staged at `~/.cache/wado/wado-relay.bin`; deploy with
  `scripts/relay-deploy.sh`. Until then the clients detect the missing `control`/`room_strict`
  caps and degrade with plain words.

- `2026-10-30` — **Autonomous run mode (user).** From here the orchestrator decides and ships without
  asking, and logs each decision here for review. Decided so far under it:
  - **Smithay is pinned in the manifest** (`rev =` in crates/compositor/Cargo.toml), which closes
    the open question.
  - **Queued order:** F2 merge → relay redeploy + F6 → F3 → F4 → F5 → platform follow-ups →
    hygiene. Hygiene runs now, in parallel, since it touches no in-flight files.
  - **An ideas backlog is kept in `plan/TODO.md`**, under "no new top-level surfaces": one search,
    continue where I left off, send/open on computer, camera scanner, Wake-on-LAN, auto
    low-bandwidth mode, battery, DND, voice actions, per-app controls, doctor card, host theme.

- `2026-10-30` — **F6 multi-client design decided; `docs/superpowers/specs/2026-10-30-multi-client-design.md`.**
  - **Sessions and encoding:** one session per daemon, with a capped sibling daemon started on
    demand. One encode per session, fanned out to every viewer at the lowest viewer's bitrate.
  - **Input and size:** a shared seat plus an input arbiter tagged per connection. The session
    keeps the lead's size, and take over resizes to the taker. Pointer lock is lead only.
  - **Who can do what:** any participant may end the session. Up to 4 devices, guarded by a link
    speed test that suggests, warns or refuses. `join`/`takeover` grants are on for code-paired
    devices. Joining needs only the grant, with a toast.
  - **Relay:** strict instance targeting (`room_strict`) ships in the same relay release as
    seatless control links.
- `2026-10-30` — **F2 signing flow:** sign the CI Pages artifact after download and redeploy it
  with `release.sig`; the key stays offline. With no release key configured, the service worker
  does not pin and accepts updates normally; pinning turns on once a key is committed (user,
  2026-10-30).

- `2026-10-30` — **Platform layer: the blend, chosen by the user from three designs;
  `docs/superpowers/specs/2026-10-30-platform-layer-design.md`.**
  - A small sync device trait on the hot path, and one typed trait per control capability.
  - A registry with honest absence (`Cap = On | Off(why)`), filled by probe-is-open providers.
  - State is pushed over `watch` channels for the GUI and web clients.
  - The isolated service wrapper is used only for video and slow D-Bus services.
  - Video declares which thread it needs, so macOS's NSApp and Slint can share the main loop.
  - The new `wado-platform` crate *is* MAC0's planned `wado-backend` boundary crate.

- `2026-10-30` — **Multi-client, a platform trait layer, and an experimental desktop app. Decided
  with the user.**
  - (1) **Multi-client: parallel sessions with co-control.** Several devices connect to one
    computer at once. Each can start its own session (its own output at its own resolution,
    alongside the others) or join an existing one. In a joined session **every device controls it
    at the same time** (pointers and keys from all of them). **Take over** moves a session to you
    and **disconnects** its previous user with a plain message; they can rejoin.
  - (2) **A perf pass and idiomatic Rust**, with host capabilities behind **traits** (input
    injection, audio, media, brightness, clipboard, capture/encode, network, ...), so that
    macOS/Windows backends and a GUI can implement them selectively. The trait shapes are chosen
    by comparing designs (`design-an-interface`).
  - (3) **Desktop app on an experimental branch: Slint, one app with two modes.** It is a host
    console (devices, pairing, sessions, config, logs, live stats) on a computer running the
    daemon, and a client to other wado computers.

- `2026-10-30` — **Computer page design approved; `docs/superpowers/specs/2026-10-30-computer-page-design.md`
  (prototype + screens in `docs/superpowers/specs/computer-page/`).** Widgets are stored on the
  phone. A light, control-only live link runs to each widget's computer and is dropped while a
  desktop session streams. Brightness gets a true level, set and read back. Wi-Fi moves under
  Connection & trust. DESIGN.md's motion system is extended with `--dur-hero`, `--dur-press` and
  `--stagger`, with no new curve.

- `2026-10-30` — **After-trackpad run: features and their intent, decided with the user
  (multiple choice).** Order: F1 → Computer page → F2 → F3 → F4 → F5, with trackpad plan 2 in
  parallel. Each is built by subagents, and the user is pinged after each one.
  - **F1 Pairing.** New devices get in by an on-screen code only: QR, or a typed short code.
    Codes are single-use and last 10 min. No first-device-wins and no remote approval. Nothing is
    shown at first daemon start; `wado pair` / the TUI on request.
  - **Computer page** (new, per host): one scrolling hub.
    - The hero shows the live link. Big actions: Desktop, Trackpad, Files, Shell. Then Now
      playing, quick toggles, and "This computer" settings: Display & session, Trackpad, Sound &
      devices, Connection & trust. All of these leave the global settings.
    - Home keeps computer cards (tap opens the page; ▶ goes straight to the desktop), running
      sessions, and app-wide settings (appearance, input feel, gamepad, diagnostics, tiles).
    - Home also gets **configurable widgets** at the end: clipboard, media player, volume,
      brightness and trackpad controls, each tied to a chosen host, with user-chosen order.
    - Heavy, deliberate motion (`motion-design`) and standout details, built with
      `ui-ux-pro-max`. DESIGN.md's motion tokens are extended rather than bypassed.
  - **F2 App.** Full P1: installable PWA, a service worker serving a pinned client, updates
    activated only when signed with the offline release key, and the version shown. Shortcuts:
    Trackpad, Files, Shell, Start/join. The icon opens the landing page.
  - **F3.**
    - A passkey guards shells, writing files, and the trackpad/keyboard. It re-locks after 15 min
      idle and when the app is backgrounded.
    - Claude: status cards; Allow/Deny from the card and the notification; a new Claude session
      in a project folder (tmux); `wado claude hooks install`.
    - Push: Claude needs permission, Claude finished, computer offline/online, transfer finished.
  - **F4.** A 64 KB/s fallback lane with a plain hint and Retry direct. Relay health via an
    external monitor, push, and the TUI/CLI. Reconnect is measured first; ICE restart only if it
    misses 2 s.
  - **F5 Pocket Console.** Shaped with mockups at the end of this run.
  - **Trackpad smoothing.** `delay "adaptive"` by default (frame interval + p98 lateness, slewed,
    8–40 ms), with `fixed` as the alternative. Supersedes the "~8 ms" default above.

- `2026-10-30` — **Trackpad plan 2 scope. Decided with the user;
  `docs/superpowers/plans/2026-10-30-trackpad-plan-2.md`.**
  - (1) **Landscape is the default way to hold it**, as a toggle, without fullscreen: upright
    phones get a 90°-rotated layout with touch coordinates rotated to match. Fullscreen with a real
    `screen.orientation.lock` is an optional extra switch.
  - (2) **Landscape layout:** an optional left rail (keys, quick actions, or none) with the pad on
    the right.
  - (3) **Settings live in a sheet inside the trackpad.** Per-device choices are stored *typed* in
    `config.kdl` (`device "<key>" { trackpad { … } }`), and a device may set its own without the
    settings grant.
  - (4) **Actions v1:** window, workspace, media, sound, system, apps. They need `host` and run as
    arguments, never a shell. **Key-combo buttons and custom commands are deferred**, which
    supersedes the spec's custom-command scope for now.
  - (5) **Phone gestures** (edge swipe in, four-finger tap, two-finger hold) are built in the same
    plan, off by default.
  - (6) **All trackpad implementation is delegated to subagents**, with the `motion-design` and
    `ui-ux-pro-max` skills for UI work.

- `2026-10-30` — **Trackpad smoothing: on by default, ~8 ms, switchable. Decided with the user.**
  The reason pinch, scroll and pointer acceleration felt unnatural is *reasoned, not yet measured*:
  libinput derives gesture velocity from event timing, and frames written to uinput on arrival
  carry network jitter. A host-side pacer now replays finger samples at a fixed 125 Hz,
  interpolating between them. It holds one frame of buffer (8 ms) on the direct link and more on
  the relay, never extrapolates, and never drops a landing or lift. It is
  `trackpad { smoothing; rate-hz; direct-delay-ms }`, with arrival-gap stats logged so the
  jitter claim can be checked. The phone also gets touch feedback (land pop, release ripple, pinch
  connector, finger-count chip), tracked 1:1 with no easing on position. Scope is also decided:
  plans 2 (actions, quick bar, settings) and 3 (phone gestures) are next.

- `2026-10-30` — **Trackpad mode, plan 1 built (branch `trackpad-mode`, merged to main).** Two
  departures from the spec, both deliberate. (1) **Every touch frame rides the unreliable
  `pad-touch` channel, down and lift included** (spec said down/up on a reliable one). Frames hold
  every finger, so the newest frame is the truth. A lost down or lift is covered by repeating the
  frame at +20 and +60 ms on the direct link, and by the daemon's 400 ms silence lift. Tap loss is
  to be measured on the phone. (2) **Held keys are released after 5 s with no message at all**
  (`KEYS_SILENCE_MS`; pings come every 2 s), and also when the direct keys channel closes. The
  phone re-sends its locked modifiers on every path change, and closing the trackpad sends `bye`
  through the relay, so devices never outlive the mode. The id `#wado-pad` was already taken by
  the gamepad overlay: the trackpad's DOM is `#wado-trackpad` / `.tp*`.

- `2026-10-29` — **Trackpad mode designed (phone as the computer's touchpad, keyboard and action
  remote; no picture, no session). Decided with the user; spec
  `docs/superpowers/specs/2026-10-29-trackpad-mode-design.md`.** Built ahead of the rest of P0 at
  the user's request. Host input through uinput: a multi-touch touchpad so libinput/Hyprland own
  every gesture, plus a keyboard. Text, dictation included, goes as text through the Wayland
  virtual-keyboard protocol. (1) **New `input` grant**, off by default for every device, legacy
  lines included: typing is terminal-equivalent and must never ride `host`. Built-in actions need
  `host`. `config.kdl` commands need `host`. Phone-created commands need `shells`. (2) **Exception
  to invariant 1:** when no direct path exists, this mode's input falls back to the relay
  WebSocket, with motion coalesced newest-wins and host-side smoothing. The user wants trackpad
  "at every situation", smooth on any connection. Measured, never claimed. (3) wado's own phone
  gestures are ones a touchpad never makes, and all are off by default.

- `2026-10-29` — **Client visual system recorded in `DESIGN.md`; a two-phase "Pocket Console, then
  Glass Cockpit" direction adopted. Decided with the user.** `DESIGN.md` (and its sidecar
  `.impeccable/design.json`) now documents the built system: base16 slots, mixed surface ladder,
  three type families, radius scale, two motion curves, float-only shadows. `PRODUCT.md` holds the
  product truth it serves. **Direction:** by default a session opens as a *Pocket Console*, with
  the picture inset at its true aspect ratio inside a shell whose controls sit around it. Going
  full screen grows the picture to the whole display, and the same controls morph into today's
  floating cockpit. This **supersedes the `2026-10-02` "never reflows / takes no space from the
  picture" rule for the windowed phase only**; full screen keeps it. The surround's contents are
  undecided (window controls, input controls, status + tiles are candidates). Not built; tracked
  in `TODO.md` GOOD-TO-HAVE.

- `2026-10-28` — **The public relay is the default; the link is always visible; a network change
  takes its own seat back. Decided with the user.** (1) `DEFAULT_RELAY` (config crate and client)
  is `wado-relay.indiasouthcentral.cloudapp.azure.com` (Azure for Students — its policy forbids
  `centralindia`). `server::selfhost` is gone: a bare `wado daemon` dials the default relay;
  `server { relay }` / `WADO_RELAY_URL` override it, `relay ""` or `wado daemon <addr>` is direct
  mode, and an argument now *wins* over a configured relay. Saved `trycloudflare.com` URLs in the
  client are moved onto the default at load. (2) **Link states**, one vocabulary on both ends:
  `offline` (with `no_network` vs `relay_unreachable`), `computer_offline`, `waiting`, `ready`
  (joined, no session — *not* "relayed": idle needs no peer connection), `connecting`, `relayed`,
  `reconnecting`, `direct` (`lan`|`internet`|`turn`, from the **selected** candidate pair).
  The client shows a badge (dot-only during a healthy direct session) and reports a
  `device_status` heartbeat (5 s and on change); the daemon logs path changes and writes
  `~/.config/wado/links/`, which `wado tui` and `wado` read. `host_net_request` → `host_net`
  carries `nat.rs`'s verdict. All forwarded-only: no relay change. (3) **Own-seat takeover.** After
  a network change the relay still sees the phone's old socket as live for up to ~45 s and would
  refuse the device its own seat. Rejected: changing the relay so a same-key join replaces a live
  link (reverses the pinned I17 test, needs a redeploy, and makes the second tab always win).
  Chosen: the client's network-change redial (`W.relayKick`) carries `takeover=1` **only when the
  device was joined at the moment of the change**; a parked or refused device redials plainly, or
  it would take *another* device's seat (found in testing). The displaced socket is told
  `taken_over` and stops, so there is no steal-war. "Takeover only on a tap" stays true everywhere
  else. (4) A session entered by **Join session** now resumes after a link drop like a started one
  (it used to wait for the dead peer connection to report `failed` — 17 s measured).

- `2026-10-27` — **The product cut: what wado is for, and the plan to make it usable end to end.
  Decided with the user in a grilling session.** Three uses — (1) the compositor, mostly for
  playing games remotely; (2) sync — clipboard, files, notifications — feeling native on the
  phone; (3) secure shells, with Claude Code session status in the UI. Work is tracked in
  `TODO.md` § **NOW — product track (PV, P0…P4)**. Decisions, by area:

  **Connectivity.** *Correction recorded first:* STUN was never the missing piece (`ice.rs` has
  three servers since September). cloudflared gave the **relay** a public address; replacing it
  means a relay on a stable host, not STUN. So: **one public relay on a VPS + domain, operated by
  the user, signalling only** — `DEFAULT_RELAY` points at it and `server::selfhost` (the
  cloudflared quick tunnel and local-relay bootstrap) is deleted. **No TURN** (no media bandwidth
  bill); `ice.rs`'s TURN hook stays so it can be added without a client change. Consequences the
  user accepted: peers that cannot hole-punch (VPN, symmetric NAT, UDP-blocking firewalls) get no
  video or files. For them, a **fallback lane**: the small messages that already ride the relay
  socket under the F0 envelope (shells, Claude status, clipboard, notifications, file listings)
  keep working, **capped at 64 KB/s per seat** so the relay is never a free TURN. The lane must be
  **unmistakable**: a persistent link badge `Direct` / `Relayed — limited` / `Offline`, and hints
  at the exact spots the cap hurts (stalled transfer, no picture, a big shell dump) saying it is
  the network, not wado, and offering *Retry direct* / "switch off VPN or to mobile data". When
  `nat.rs` or ICE names a cause, the wording names it; otherwise it blames nobody. The UI splits
  **online** (relay sees the daemon) from **connected** (ICE succeeded) — a green dot must not lie.
  **Network change:** fast fresh reconnect (new peer connection over the still-open relay socket,
  session untouched), target **< 2 s to picture behind a "Reconnecting…" overlay**; an ICE-restart
  spike on webrtc-rs 0.17 only if that misses. Relay ops: **single VPS + uptime monitoring**, the
  client says "wado's relay is unreachable — not your computer"; multi-relay stays GOOD-TO-HAVE.

  **Trust.** *First-device-wins is removed* — on a permanent public relay the ~30-bit Remote ID
  would be the only thing between the internet and a fresh install. A device gets in **only by
  scanning a code on the computer** (QR or pairing code, **single-use, 10 min**, was 24 h) **or by
  approval from a device that already did.** **Shells: passkey-or-nothing** — WebAuthn step-up
  (registered at pairing, ES256 check against a key stored beside the `trusted_clients` line),
  re-locks after **15 min idle** or when the page backgrounds; any authenticator accepted, none ⇒
  no shells. Plus a per-device shell audit log and a live "who is attached" view in the TUI with
  a kick key. **Client trust root:** the service worker serves a pinned client build and only
  activates an update signed by an **offline release key** (not in CI); the user sees the version
  change. One bad push can no longer own every user.

  **Shells + Claude.** Status from **Claude Code hooks**, installed by an explicit opt-in that shows
  the diff to `~/.claude/settings.json` (never automatic); states working / needs permission /
  waiting / finished / ended. Cards are metadata only, gated by the `shells` grant **without** the
  passkey (answering goes through a shell, which is passkey-locked), and ride the fallback lane.
  Tap a card → its wado shell. **"New Claude session"** runs `claude` in a **tmux** session wado
  creates (survives daemon restarts, attachable at the desk); wado does **not** attach to the
  user's own tmux panes. Claude outside wado: status + a hint.

  **Notifications.** **Web Push**, opt-in per kind per device; default on = *Claude needs
  permission*; Claude finished/errored, session-app notifications opt-in. iOS needs the PWA
  installed. Host-desktop mirroring: not wanted. Phone → computer: impossible from a browser,
  recorded for the Android app (M6). Camera auto-upload: dropped.

  **Files.** One wado style with **native physics** (virtualised momentum list, edge-swipe back,
  pull-to-refresh, long-press select, haptics, prefetched thumbnails, < 150 ms cached folder
  paint). The service worker that Push requires also **streams Safari downloads** (supersedes the
  F2 "no service worker" entry for that case). **Share → wado** on Android (Web Share Target).
  **Listings and files ≤ 1 MB on the fallback lane**; thumbnails and big transfers wait for Direct.

  **Clipboard.** Password-manager copies (`x-kde-passwordManagerHint: secret`) are **never synced
  or listed**. `cliphist` when present, else wado's own **RAM-only last-50** history. Phone →
  computer: **auto-offer on focus** where the browser allows a silent read (Chrome), a tap
  elsewhere (Safari's paste bubble would nag).

  **Gaming.** In scope: **physical controllers** (Gamepad API → own unreliable newest-wins channel
  → uinput), **udev rule shipped by the packages**, **NVIDIA** (`nvenc-cpu` first, then a measured
  spike CUDA-GL interop vs Vulkan Video before any zero-copy work), **M5 per-device output**. A
  **first-run encoder self-test** (`wado doctor`, auto on first daemon start, cached per GPU +
  driver) picks the tier and the highest resolution/fps the machine *sustains*, and says so in
  plain words. **Steam already open on the host**: detect and *offer* to move it (never kill
  silently). Rootless XWM: later, on demonstrated breakage. Verification: the user's NVIDIA,
  Intel and Apple-silicon machines, rented cloud GPUs per milestone, a CPU-only VM for the
  software tier's release fps, and beta testers via `wado doctor --report`.

  **Packaging.** Nix + `.deb` / `.rpm` / AUR, each with the udev rule and a `systemd --user` unit
  (daemon reachable after login; sessions still start only on demand). **ffmpeg 8 built in CI
  and linked statically**; Mesa / libva / the NVIDIA driver always from the host.

  **Order.** **PV** — a phone verification sprint over the ~25 awaiting-verification items, so
  new work is not built on unconfirmed UI — then **P0** connect & trust → **P1** shells, Claude,
  service worker + Push + signed client → **P2** clipboard + files → **P3** gaming → **P4**
  packaging. Every phase ends with a real-phone checklist.

- `2026-10-17` — **A macOS host backend, model (B) by necessity. Decided with the user.** A Mac
  cannot host a third-party compositor, so on macOS wado *mirrors* a per-device virtual display
  instead of being the compositor: ScreenCaptureKit capture, VideoToolbox H.264, CGEvent input,
  and the private `CGVirtualDisplay` API to give each device a display at its native size —
  which is what keeps invariant 8's promise (1:1 mapping, no letterboxing) on a Mac. The design
  follows Sill (github.com/Saffsanity/sill, Apache-2.0, Swift), studied for design and measured
  traps, not copied. Linux stays model (A); nothing about it changes. Accepted costs: **touch is
  pointer emulation on macOS** (there is no public touch-injection API), the virtual display is
  a private API that any macOS release can break (so plain main-display capture is the
  always-working fallback, the analogue of invariant 3), and Screen Recording + Accessibility
  must be granted. The server keeps the backend boundary; the boundary types move out of the
  Smithay crate so a second backend can implement them. Plan: Milestone MAC below.

- `2026-10-13` — **The relay moves to a VPS; the quick tunnel becomes the fallback. Decided
  with the user.** A `trycloudflare.com` hostname is minted per cloudflared process, so every
  reboot or tunnel heal rotated the URL and every phone had to rescan the QR. Device trust already
  persists, so the URL was the only thing that needed to stay put. Picked over a Cloudflare named
  tunnel (needs a domain on Cloudflare) and Tailscale Funnel. Debian/Ubuntu host with
  `deploy/relay/` (Caddy + systemd) as-is. A daemon with `server { relay }` set already skips
  the local relay and tunnel (`selfhost::prepare`), and `public-relay` goes into the QR. Once it
  is up, `DEFAULT_RELAY` points at it. No TURN yet: cloudflared never carried media either, so
  this changes nothing for NAT traversal.

- `2026-10-06` — **Clipboard rail: the host desktop's clipboard, synced with the session.
  Decided with the user.** The landing page has a vertical rail (right edge; a drawer behind an
  edge tab on narrow screens) showing the *host desktop's* clipboard history — cliphist, read with
  the desktop's `WAYLAND_DISPLAY` captured before the compositor overwrites it
  (`server::clip`). Text, links and images; pin (wado's own `clip_pins`, by preview), delete,
  search, kind chips, tap to copy to the device, and **Send mine** pushes the device's clipboard
  (text or image) onto the host with `wl-copy`. Host ⇄ session are kept in step both ways (a hash
  stops the echo); the compositor clipboard is now mime + bytes, not text. Gated by a **new
  `clipboard` grant** — the history holds passwords; the owner always has it. No "clear all"
  (not wanted). Needs a Wayland desktop and cliphist; without either the rail says why.

- `2026-10-06` — **The `wado` binary hosts its own relay and tunnel; `rig.sh` is no longer
  needed to be reachable. Decided with the user.** A bare `wado daemon` (no `server { relay }`, no
  listen address) used to fall back to direct mode on localhost, which no phone can reach. It now
  (`crates/server/src/selfhost/`) starts `wado-relay` beside it on :4000 and a cloudflared quick
  tunnel if they are not already up — detached, shared by every daemon — dials the relay via
  `WADO_RELAY_URL`, and takes the next free instance/UDP slice, so a second `wado daemon` joins the
  pool. A watchdog (one daemon, by `tunnel.lock`) checks cloudflared's local `/ready` every minute
  and replaces the tunnel after 3 failures: a quick tunnel can die under a running cloudflared
  ("Tunnel not found") with everything behind it healthy. Replacing rotates the URL; rescan the
  QR. `wado tui` with nothing running starts one `wado daemon`, not `rig.sh`. Direct mode remains
  `wado daemon <addr>`; a configured relay is used as before.

- `2026-10-05` — **`wado` CLI, `wado tui`, and per-device grants. Decided with the user.**
  Bare `wado` prints a landing page (version · git hash · build time, what is running, the
  commands); the daemon is `wado daemon`. `wado tui` is a ratatui control panel in the lazygit
  shape (numbered panels left: Rig, Sessions, Devices, Pair, Security; detail and an activity feed
  right; approvals as a modal) with btop touches (live pool gauge — real numbers only). It reads
  the same files the daemons share and writes `trusted_clients` and `ui.kdl`, so it never disagrees
  with a phone's settings; with nothing running it starts the rig (`scripts/rig.sh --headless`).
  `rig.sh` now ends in the TUI; `--headless` keeps the old printout. `wado-relay` keeps clap, with
  styled help and the hash in `--version`; no TUI. A fish function runs the latest release build
  through `direnv exec` (the daemon needs the dev shell's graphics env).
  **Grants are per device** (`gate/grants.rs`): files ro/rw (column 5, still needs a QR pin),
  `shells`, `settings` (privileged config, Wi-Fi), `host` (sound, Bluetooth, sleep) in column 6. A
  line without column 6 keeps what it had before: shells + host. The owner has everything but
  files. A pairing code carries a checklist and redeeming it *adds* those grants (never removes).
  `security { new-device }` is what a newly trusted device starts with (default `host`). This
  **supersedes** `files-default` and `shell-access` below, which were removed the same day.

- `2026-10-05` — **Permission switches in `security { }`, decided with the user.** `join "ask"`
  (default) keeps the approval rule of `2026-10-02`; `join "open"` lets any device in as if
  approved *once* — never written to `trusted_clients`, so it gets no files (a once device never
  does) and no shells, and setting `join` back to `"ask"` shuts the door with nothing to prune.
  It never applies to a takeover, which still needs someone's say-so. It is for testing: anyone
  who reaches the remote id gets the desktop. `files-default none|ro|rw` is the grant a newly
  trusted device starts with; it still counts only once QR-pinned. `shell-access "trusted" |
  "owner"` gates open, input, resize, close, list and output of the daemon-owned shells — they
  outlive connections and hold the owner's ssh sessions, so a once device never sees them (a
  tightening: before this, a device approved once got them). Ceiling: a desktop guest can still
  start a terminal *app* in the compositor; the gate covers the console, not the desktop.
  Honouring file grants without a QR pin was **deferred** — the user is designing that path.

- `2026-10-05` — **Permission switches in `security { }`, decided with the user.** `join "ask"`
  (default) keeps the approval rule of `2026-10-02`; `join "open"` lets any device in as if
  approved *once* — never written to `trusted_clients`, so it gets no files (a once device never
  does) and no shells, and setting `join` back to `"ask"` shuts the door with nothing to prune.
  It never applies to a takeover, which still needs someone's say-so. It is for testing: anyone
  who reaches the remote id gets the desktop. `files-default none|ro|rw` is the grant a newly
  trusted device starts with; it still counts only once QR-pinned. `shell-access "trusted" |
  "owner"` gates open, input, resize, close, list and output of the daemon-owned shells — they
  outlive connections and hold the owner's ssh sessions, so a once device never sees them (a
  tightening: before this, a device approved once got them). Ceiling: a desktop guest can still
  start a terminal *app* in the compositor; the gate covers the console, not the desktop.
  Honouring file grants without a QR pin was **deferred** — the user is designing that path.

- `2026-10-04` — **File manager (browse, upload, download), and the end-to-end security it
  needs first. Decided with the user in a grilling session.** Facts that set it, read from the
  code that day: every daemon↔client message after the relay handshake (`SessionLaunch` with its
  free-form command, PTY shells, host controls, approvals, SDP) crosses the relay WebSocket as
  **plaintext JSON** (`relay_link.js` `relaySendMsg`, `relay_client.rs` dispatch); the QR pair
  code rides the WebSocket URL (`&pair=`), so the relay sees it; `client_key` is a random id
  the relay passes along and could forge; nothing binds the SDP's DTLS fingerprint to the host.
  A hostile relay or tunnel can therefore already inject commands, not just watch — files would
  add exfiltration. Decisions:
  1. **F0 — an end-to-end envelope over the relay, before any file code.** Supersedes "sign the
     SDP" from earlier in the same session, which left the control plane open. On join the
     client and daemon run a SIGMA-style handshake through the relay: each sends an ephemeral
     X25519 key, each signs the transcript with its long-term **Ed25519** key, both derive keys
     with HKDF-SHA256. Every daemon↔client `RelayMsg` after that is sealed **AES-256-GCM** with
     a per-direction counter (replay and reorder fail closed). AES-GCM rather than ChaCha20 because
     WebCrypto has it natively and the daemon already links `ring`. Frames addressed to the
     relay itself (`pong`, `peer_accept`/`peer_reject`, `seat_hold`, `occupancy`, `leave`) stay
     plaintext; the relay sees `{"type":"sealed",…}` for the rest and forwards it unread —
     **no relay change**, the frozen `relay_wire` is untouched. The SDP is inside the envelope,
     so the DTLS fingerprint is authenticated by it with no separate signature.
  2. **Keys.** Device: a WebCrypto Ed25519 key generated **non-extractable**, kept in IndexedDB —
     page JS can sign with it, never read it. Host: one Ed25519 key in the shared config dir
     (`host_key`, 0600), so every daemon of a pool presents the same identity. *As built:*
     `client_key` **stays the browser's random id** — a seat id and trust-list line, no longer a
     secret — and the trust list gains the device's key beside it
     (`<client_key>\t<name>\t<device pk>\t<pinned>`). `Gate::admit` decides at `e2e_finish` on
     the key the device proved, never on the id the relay named; a line without a key takes the
     first one proven for it. Simpler than the planned hash-of-key id, which would have needed a
     migration of every seat and trust line, and equally strong. `e2e_finish` itself travels in
     the clear (public key + signature; the relay learns nothing it did not know from
     `client_key`); everything after it is sealed.
  3. **Pinning.** The QR link (`wado qr`) carries a hash of the host key; scanning it pins it.
     The pair code **never leaves the device** again: it is proven with an HMAC over the
     handshake transcript. The relay's `pair` field shrinks to a non-secret "I have a code"
     marker (`pair=1`): it lets a fresh join through to prove the code, and **never** counts for a
     takeover — a claim alone must not displace a live viewer. A join without a pinned host key
     (typed Remote ID) trusts the host key on first use and is marked unpinned; a pinned host
     key that changes is a hard stop on the client, with no automatic redial.
  4. **Migration.** The client is auto-deployed, so no old client code survives a push; old
     *devices* do. A trust-list entry holding a legacy UUID is converted on that device's first
     sealed handshake (trust on first use, one connection's window) and keeps viewing and
     control. It gets **no file access** until re-paired by QR.
  5. **Client hardening.** The hosting origin is trusted (2FA on the repo). *As built:* **SRI
     hashes on the CDN assets (xterm) instead of a CSP.** A CSP here would have to allow
     `eval` (the bridge is one Dioxus eval) and any `wss:` host (the relay URL is user-editable),
     so it would block almost nothing; the CDN scripts, unpinned, could have run arbitrary code
     next to the device key. Revisit a CSP once the relay host is fixed.
  6. **Permissions.** `trusted_clients` gains `files=none|ro|rw` per device, default `none`;
     granted with `wado files grant <device> ro|rw|none` on the host or from an `rw`-granted,
     QR-pinned device. Devices approved "once" never get files.
  7. **Scope.** `config.kdl` `files { root …; deny …; hidden; follow-symlinks "inside" }`. Default
     root is home; `/` only if written. Default denylist: `~/.ssh`, `~/.gnupg`,
     `~/.config/wado` (so a device cannot grant itself more), `~/.local/share/keyrings`. Every path
     is resolved with `openat2(RESOLVE_BENEATH)` under its root; deny is applied after resolution.
  8. **Transport.** Files get a **second RTCPeerConnection**, opened on demand, signalled through
     the envelope. Its SCTP association is separate from input's (`3ca6a94` showed extra input
     copies alone congest a shared one), so invariant #1 holds. File traffic has a rate cap that
     backs off when video reports loss. One ordered, reliable data channel; 256 KiB chunks with
     `bufferedAmountLow` flow control.
  9. **Transfers.** A queue, two at once, each pausable or cancellable; speed and time left on
     every bar. Uploads write `<name>.wado-part`, renamed after a hash match. Both directions
     resume from the last confirmed offset after a reconnect. Downloads land in **OPFS** (on
     disk, survives a reload) and a service worker streams the finished file into the browser's
     own Downloads — no in-memory Blob, no size ceiling but disk. A multi-select is separate,
     resumable files; a folder is a STORE (uncompressed) zip stream that restarts on a drop.
     Uploads take files and whole folders; a name clash asks Replace / Keep both / Skip, with
     "apply to all". Refused up front if free disk minus a reserve is too small.
  10. **Operations in v1:** new folder, rename, delete **to the freedesktop Trash** (never a hard
      delete from a phone), move and copy within scope.
  11. **Quick access.** Pins are the GTK bookmarks file (shared with Nautilus) plus the XDG
      dirs. Recents merge `recently-used.xbel` with a bounded mtime scan of the pinned dirs.
      Both are filtered through the scope rules. Server-side, the same for every device.
  12. **UI.** Bundled SVG icons by MIME family, coloured by the theme; thumbnails from the
      host's freedesktop thumbnail cache, made on demand for images only, lazy-loaded. Below
      600 px a Google Files layout (quick-access home, long-press select, a floating button);
      wider, a Nautilus layout (sidebar, list/grid, sortable columns, drag-drop upload). In a
      session it is a sheet over the stream with a progress pill in the bar, and transfers keep
      going when it closes. On the landing page a **Files** tile opens it with no session — no
      compositor and no encoder start.
  13. **Audit.** Every operation goes to the daemon log and an append-only
      `~/.local/state/wado/files.log` (time, device, op, path, size, result). Other connected
      devices of the pool get a toast.
  14. **Order:** F0 envelope → F1 browse (second PC, scope, grant) → F2 download → F3 upload →
      F4 file ops → F5 pins, recents, thumbnails, toasts. Each stage ships alone.
  **F0 as built (`2026-10-04`):** `crates/protocol/src/envelope.rs`, `crates/server/src/e2e/`
  (`host_key`, `seal`, `handshake`, `link` — the writer task is the one sealing point),
  `crates/client/src/js/e2e_{keys,seal,handshake}.js`. Found while building: five client files
  wrote to the relay socket directly, bypassing `relaySendMsg` (launch/window actions, apps,
  timing, tap targets, menus) — harmless in plaintext, silently dead once sealed.
  `relay-link-check.mjs` now fails on any such write. Second find: a close must be synchronous
  when nothing is queued, or a drop-then-redial races the relay's "second tab" guard.
  Verified: unit tests (cross-checked against WebCrypto by `scripts/e2e-vector.mjs`), the
  hostile-relay cases in `relay-seat-e2e.mjs`, and `pool-e2e`/`session-e2e`/`shells-e2e` in
  real Chrome over the envelope. **Not verified: a phone's browser** (WebCrypto Ed25519/X25519
  needs Chrome 137+, Firefox 130+, Safari 17+).
  Code lives in `crates/server/src/files/` (one job per file) and a `files` schema in
  `crates/config`; the compositor never sees it.
  **F1–F5 as built (`2026-10-04`):** server `crates/server/src/files/` (`scope`, `channel`,
  `list`, `send`, `zip`, `recv`, `ops`, `trash`, `quick`, `thumb`, `audit`, `pace`), wire
  `crates/protocol/src/files.rs`, client `js/files_{link,sha,icons,xfer,ui,panels}.js` +
  `assets/files.css`, CLI `wado files`. Departures from the plan, each simpler and no weaker:
  - **No service worker.** OPFS's `getFile()` is already a disk-backed `File`; an object URL of it
    and an `<a download>` hand it to the browser's Downloads without reading it into memory. Same
    property (no size ceiling but disk), one moving part fewer.
  - **60 KiB chunks, not 256 KiB**: webrtc-rs 0.17's SCTP caps a message at 64 KiB. Flow control
    is a 1 MiB high-water mark on `bufferedAmount`, both ends.
  - **Pause is cancel + resume.** A download resumes from what OPFS holds, an upload from its
    `.wado-part`; both hash the whole file at the end (the prefix is re-read), so a resume across
    two versions of a file is caught. The client hashes with an incremental SHA-256
    (`files_sha.js`) because WebCrypto cannot hash a stream.
  - **Every mutation is on descriptors**: the parent is resolved with `openat2(RESOLVE_BENEATH)`,
    the one checked name acted on with `openat(O_NOFOLLOW)`/`mkdirat`/`renameat2(NOREPLACE)`.
    A tree that *contains* a denied one cannot be moved, renamed or trashed (it would carry it
    out); zip and copy skip denied subtrees. The built-in denylist cannot be removed by config.
  - **Replace sends the old file to the Trash**, for uploads as for move/copy — never a clobber.
  - **Zip without Zip64**: a folder over 4 GiB or 65 535 entries is refused with "download its
    files instead". Size is exact up front (bit-3 data descriptors carry the CRCs).
  - **Thumbnails**: the shared freedesktop cache is read; new ones are made by
    `gdk-pixbuf-thumbnailer` (if present) from `/proc/<pid>/fd/<n>` of the scope-checked file,
    and kept in `~/.cache/wado/thumbs` rather than written into the shared cache.
  - **Toasts across the pool** ride the audit log: every daemon follows `files.log` and tells its
    own viewer about other daemons' operations — no daemon-to-daemon channel.
  - **Grants need a QR-pinned key line** (`pinned = 1`) as well as `files=rw|ro`; the 5th column of
    `trusted_clients`. Checked per request, so a revoke applies to an open channel at once.
  - **Download pacing**: a process-wide token bucket at `files.rate-with-video` Mbps while a
    session's video peer connection is up, halved for 5 s after an RTCP receiver report with
    fraction lost > 5/256 or while the viewer reports strain. Uploads are not paced (the
    viewer's uplink carries no video).
  **Viewer (`2026-10-05`, `js/files_view.js`):** browser-native renderers only — no viewer
  library to trust next to the device key. A blob: URL carries this page's origin, so HTML and
  SVG-as-document are **never** rendered (source text / `<img>` only); that rule is load-bearing.
  Bytes come whole over the files channel (hash-checked) — no streaming yet; tier 2 in TODO.md.
  **Streaming and the revision (`2026-10-05`):** video and audio no longer arrive whole. The daemon
  (`files/media.rs`) runs `ffmpeg` to fragmented MP4: a remux when the codecs are H.264(+HEVC if
  the device decodes it)/AAC, a libx264 `veryfast` transcode otherwise. The client
  (`files_stream.js`) feeds it to Media Source Extensions. Seeking restarts ffmpeg with `-ss` +
  `-output_ts_offset` so fragments keep the film's timestamps. Pacing is credit (4 MiB grants
  while < 60 s is buffered), and ffmpeg blocks on its pipe. **Containment, so files can never hurt
  a session:** every helper runs through `files::tool` (nice 19, `PR_SET_PDEATHSIG`,
  kill-on-drop); at most 2 transcodes per daemon at half the cores; a stalled stream is killed
  after 45 s; stderr is drained so a noisy file cannot block ffmpeg. Player UI is **Plyr**, photo
  zoom is **Panzoom** — the user asked for robust prebuilt libraries — both MIT, SRI-pinned
  in `index.html`, Plyr's sprite vendored so nothing is fetched unpinned. Subtitles are converted
  to WebVTT by ffmpeg and drawn by our own code (delay is arithmetic; textContent only).
  The window was revised as a product: an overview (search, categories via a bounded `find`,
  recents, storage from `hello.space`), grid in both layouts, open-on-computer (a
  single-quoted `xdg-open` through the existing session launch — no new privilege), share,
  properties, Trash view + `restore`. Verified by `files-e2e.mjs` (streaming, seek, captions,
  audio tracks, nice 19, ffmpeg gone after close) and pool/shells e2e unchanged.
  Resolved open items: a Files-only connection is answered by whichever daemon holds the seat
  (they share the config dir, so grants and scope agree); uploads take the daemon's umask.
  Verified: `scripts/files-e2e.mjs`, 57 checks in two real Chromes against a pool of two daemons,
  plus unit tests (scope escapes, ops, trash, zip size, grants, pace). **Not verified on a
  phone's browser.**

- `2026-10-03` — **Latency compass, decided with the user in a grilling session.** Facts that set
  it: the daemon adds < 5 ms (capture 0.1, encode 3.3, queue 0, write_sample p99 0.3); the media
  path is direct P2P (srflx), rtt floor 11–30 ms; the phone decodes in 8–10 ms; **Chrome's playout
  buffer regressed from 7–14 ms (2026-09-12, no audio track) to 55–400 ms** at a similar rtt. The
  prime suspect is A/V lip sync: video and audio share the stream id `"wado"`, so Chrome holds video
  to the audio jitter buffer. "The browser only allows part of the bandwidth" was examined and is not
  so: the receiver does not cap, webrtc-rs 0.17 has no congestion control, and the ceiling is our own
  bitrate choice. Decisions:
  1. **A/V stay in sync, always** — the user rejected splitting the sync group outright. So the audio
     path is what must get faster: video can only be as fast as audio.
  2. **Phase 1, quick wins:** measure the audio leg (audio jitter buffer, `jitterBufferMinimumDelay`
     as the sync hold, daemon capture buffering, Opus frame size); trim the daemon side (smaller
     PipeWire quantum, 10 ms Opus frames, no batching); trim Chrome's audio buffer
     (`jitterBufferTarget = 0` on the audio receiver, DTX off). Tester visibility: per-leg line +
     culprit extended with audio and the sync hold; input round trip always on; a pipeline A/B badge.
  3. **Phase 2, server-side congestion control** (R6): estimate from TWCC feedback in the daemon,
     target **~85% of the estimate**, capped per resolution (~12 Mbps at 1080p). Replaces the
     client auto bitrate as the controller.
  4. **Phase 3, a WebCodecs receiver** — hardware `VideoDecoder` (`optimizeForLatency`) and Web Audio,
     frames over an **unreliable, unordered WebRTC data channel** (still P2P/UDP, so invariant #2
     holds), with our own small shared A/V buffer. Loss: **NACK + keyframe request, and FEC (WASM,
     e.g. Reed-Solomon) as a switch** — A/B both and their combination. **Opt-in**, today's WebRTC
     path stays the default and the automatic fallback. **Android Chrome first**; desktop Chrome
     follows, Safari later on the old path. WASM is for FEC/reassembly, not decoding: software H.264
     on a phone is slower and costlier than the hardware decoder WebCodecs reaches.
  **Rule (the user, same session): every new optimisation that changes or could break existing
  behaviour ships as an option on the client's Display & stream page**, so a tester can switch it
  on and off and A/B it: low-latency audio (Phase 1 trims), the server estimator (Phase 2), and
  the WebCodecs pipeline with its NACK / FEC modes (Phase 3). Auto bitrate already follows it.
  Every phase is measured the same way: `scripts/perf-report.mjs` (protocol in its header), rows
  in `plan/data/perf-runs.jsonl`. Baseline row: `baseline-before-latency-work-4g`.
- `2026-10-03` — **Auto bitrate, decided with the user: the viewer caps the encoder to what its
  link carries.** Measured live that day: a session sending 3.9 Mbps into a cellular path that
  had collapsed to ~0.5–1.9 Mbps (rtt 24→360 ms) shed to 22 of 90 fps and still arrived in bursts,
  with the browser holding ~270 ms of playout buffer to smooth them. Shedding (`congestion.rs`) is
  the only lever the server had, and it does not fix arrival. Now `js/autorate.js` reads rtt
  against its 30 s floor, loss, jbuf and the received rate (not `availableIncomingBitrate`, which
  was measured off by up to 76×). After 3 congested seconds it caps to ~90% of the best received
  rate (at most halving); after 10 clean seconds it climbs +50% to the full rate; it changes at
  most once every 6 s. The cap travels as `SessionConfig::bitrate_cap_kbps` through an ordinary
  reconfigure, which rebuilds only the encoder (output, windows and apps kept; ~15–25 ms, one
  IDR). It is on by default and switchable (Display page). Not a replacement for real congestion
  control: research R6 still stands for a server-side, per-packet estimator.
- `2026-10-03` — **Host controls: the computer's sound, Wi-Fi, Bluetooth and sleep, from the
  viewer, over the relay, with no session needed** (`server::host`).
  - **The "This phone" sink is per daemon, not per session.** It is created at daemon start
    (`host::phone_sink`) and handed to the compositor as `CompositorCommand::AudioSink`. That
    makes routing the host's own playback to the phone possible in shell-only mode. Capture
    runs only while a peer connection carrying audio is up. A shell-only listener uses an
    audio-only offer, and the daemon answers with only the tracks offered.
  - Tools, not libraries: `pw-dump` / `wpctl` / `pw-metadata` (moving a stream is setting its
    `target.object`), `nmcli`, `bluetoothctl`, `systemd-inhibit`. Arguments are passed as
    arguments, never through a shell. Other daemons' phone sinks are hidden from the list.
  - **Wi-Fi changes are on probation**: joining is undone after 60 s unless a viewer sends
    `WifiKeep`, and off can be time-limited. Wi-Fi is owner-only; sound, Bluetooth and sleep
    need only a trusted device.
  - **Home is "show desktop"** (`compositor::desktop`): every window is unmapped with its place
    kept, and focus is cleared. The window list still includes hidden windows, so the switcher
    can bring them back. The dock is ⋯ · Apps · Home · Back.
  - The tap lens was removed (user report: it did not work as intended). Taps snap only when
    exactly one accessible target is near.

- `2026-10-02` — **The 2026-10-02 redesign as built: where it differs from what was decided.**
  All of C1–C3, U1–U2 and F1–F4 are implemented. These are the deviations, each deliberate:

  1. **Trust stays in `trusted_clients`, not in config.kdl.** It is state that approvals write at
     runtime, and a pool shares it. Only *policy* moved: `security { trust-first-device; owner }`.
     Nothing was migrated, because there was nothing to migrate.
  2. **Device prefs are an opaque blob** (`device "<key>" { prefs "<json>" }`). The client owns
     its settings' shape; typed keys are the upgrade if people hand-edit them.
  3. **Shells outlive sessions too, not just connections.** Shell-only mode has no session, so
     tying shells to one would kill them at the wrong time. A shell ends when it exits or its
     tab is closed. Eight at most.
  4. **Apps follow light/dark through env** (`GTK_THEME`, `ADW_DEBUG_COLOR_SCHEME`), not through
     a settings portal; the portal is the upgrade (accent colour, Qt and Electron).
     wado draws no decorations, so "SSD from base16" had nothing to apply to.
  5. **Screenshot and record are client-side**: a canvas draw and a MediaRecorder on the stream
     the page already has. No daemon work.
  6. **Binds have no `spawn`**: a bind that runs commands would make `binds` privileged.
  7. **Gestures are three-finger swipes, in gesture mode only.** In raw-touch mode three fingers
     belong to the app.
  8. **Blur is decided by a pointer media query**, not by a measured device-drop rate.
  9. **Audio uses a per-session PipeWire null sink plus `PULSE_SINK`.** Native-PipeWire apps that
     ignore `PULSE_SINK` still play on the host. The sink name carries the daemon's pid, and
     `PR_SET_PDEATHSIG` removes it if the daemon dies.
  10. **Config edits travel over the relay only** (as the PTY already did). Direct mode reads
      config.kdl but cannot edit it from the client.

- `2026-10-02` — **Client redesign + a compositor-style daemon config. Decided with the user
  in a grilling session (Q1–Q31); prototype at https://claude.ai/artifact/6FsPf486ZAiHU9ubbSsHui.**

  *Shell.* One **control centre** sheet replaces the docked settings column *and* the ⋯ pop-grid
  on every screen size. It floats over the video and never reflows it (a bottom sheet on phones,
  a right-side card on desktop). A pre-session **landing** shows saved **host profiles**, each
  holding its connection and its own last session shape (client localStorage, because the host
  list is needed before any daemon is reachable). It also has the 4-hop progress ring, *Start*,
  and *Open shell* (shell-only mode: data channel, no compositor or encoder). **Quick tiles**:
  a default 8 plus an edit mode drawing on a pool (stats overlay, keep awake, lens, resync, and
  each new feature's tile). Tap toggles, long-press opens the tile's page. **Pages are by
  subject**, and each control carries an *applies live / on Apply / at restart* badge. A sticky
  Apply bar appears only when a reconfigure is pending. Resolution is shown as aspect-drawn
  chips plus *More…*. Long hints shrink to one line plus an ⓘ expander.
  **Invariant #5** is met by the dock health LED, a start toast and a hero chip, never a reflowing
  banner. Theming: about 12 base16 schemes, light schemes allowed, while the stage behind the video
  always stays dark. Elevation is mixed from base00 and base05. Appearance adds radius, motion
  level and accent slot. Blur is automatic: on for desktop, off for phones and when the drop rate
  is high.

  *Shells.* Multiple PTYs per viewer, owned by the **session (or shell-only host), not the
  connection**. Tabs survive a drop and reattach with a scrollback replay. SSH tabs are PTYs
  running `ssh <alias>` **on the host**, using its keys and its `~/.ssh/config` host list.
  There is no SSH in the browser.

  *Config.* `~/.config/wado/config.kdl` (KDL, niri-style), with a new `wado-config` crate
  (serde types + per-key metadata; no smithay or tokio) shared by the server and the WASM
  client. The **daemon config sets defaults and hard limits**, and the client picks inside the
  limits. The client **can edit the config too**: live sections (input, rules, binds, theme,
  session, stream) from any trusted device; privileged ones (server, security, autostart, env)
  only from an *owner* device, after confirmation. Client edits go to a wado-owned `ui.kdl`
  included from `config.kdl`; the hand-written file is never rewritten. Per-device prefs live in
  `device "<name>" {}` blocks. The file is watched and hot-reloads; a bad parse keeps the last
  good config and reports line and column. Env `WADO_*` still overrides the file. On first run,
  `trusted_clients` and `remote_id` are imported. CLI: `wado validate`, plus `wado msg` over a
  unix socket. Sections: server, security, encoder/stream, session, input, window-rule, shells,
  binds + gestures. Session apps follow the theme: color-scheme and accent through a settings
  portal, and wado-drawn decorations.

  *Order.* C1 config core → C2 hot reload, write-back, msg → U1 shell → U2 shell tabs → C3 binds,
  rules, app theming → F1 clipboard → F2 notifications → F3 screenshot/record → F4 audio.
  The UI never shows a control that does nothing; each feature lands its tile with it.

- `2026-10-02` — **Relay finalisation steps 3–7 built, and four design points that differ from
  the plan.** Seats are implemented as planned: parked joins, held seats keyed by `client_key`,
  one-tap takeover, the approval gate, two-way liveness (15 s ping, 45 s silence) and a per-IP
  token bucket (20, one back per 3 s). `scripts/relay-seat-e2e.mjs` runs all of it against the
  real binaries, with 31 checks.

  1. **Approval requests travel through files, not the relay.** A pool gives a new device a
     *free* daemon, which by definition has no viewer to ask. The approver sits on another
     daemon of the same pool. Routing prompts between rooms would have put policy back in the
     relay, so the daemons share `~/.config/wado/{trusted_clients,pending/}` instead (one machine,
     one config dir). Each daemon shows pending requests to its own approved viewer.
  2. **Trust on first use.** "Known keys get in; unknown wait for a connected device" leaves
     the very first device with nobody to ask. While the trust list is empty, the first device
     with a key is trusted. A client with no key is never trusted.
  3. **A takeover asks the daemon before it displaces anyone** (`peer_check`). As first written,
     the displacement came before the gate. Anyone holding the Remote ID could then knock a
     viewer off, and the only device able to approve them was the one just told not to
     reconnect. Now an untrusted takeover is put to the viewer it would displace.
  4. **No `peer_away` / `peer_back` and no `leave` message.** When the daemon drops, the relay
     closes the client's link and the client parks on its held seat. That reuses the parking
     path rather than adding two messages. `pagehide` does not free a seat: on a phone it fires
     on every app switch, which is exactly when the seat should be held.

  **Known limits.** A seat whose session was ended with *End session* is still held for its
  device until the hold expires. Another device then sees "in use" and can take it with one tap.
  Nothing tells the relay a session ended, because that would mean parsing a session message.
  An expired held seat is reclaimed only when the next join touches that instance.

- `2026-10-02` — **The relay is finalised around a *seat* that outlives both sockets; the
  handshake is frozen so later daemon versions never need a relay redeploy.** Goal stated by
  the user: deploy the relay once and keep it there across major daemon releases, and have
  client and daemon find each other whenever both are up, whichever came first.

  **Seat model.** A seat is keyed by `(remote_id, instance_key)` and has a daemon half and a
  client half, each `Live`, `Away(since)` or absent. When there is no daemon, the join **parks**
  on its open socket (a `waiting` message, then `join_accepted` when a daemon registers),
  replacing `join_denied` + client polling. If either half drops, the seat is **held** for the
  grace period and the other side is told `peer_away`, then `peer_back`. A client may reclaim
  a held seat only when its own half is `Away`, **never `Live`**, so the I17 rule (nothing
  takes a room from a live client) stands unchanged. A `pagehide` → `leave` frees a seat
  immediately. *Built as* a WebSocket close code (`LEAVE_CLOSE_CODE` 4001, cap `leave`), not a
  message, so the relay still reads no client frame; the client sends it on `relayDrop` and on
  a `pagehide` with **no session running** (with one, `pagehide` may be an app switch and the
  hold is what lets it resume).

  **Stable identity:** `instance_key` (sent in `register`, stable across daemon and relay
  restarts) replaces the per-connection uuid; `client_key` (browser `localStorage`) replaces
  `?instance=`. Together they let the relay rebuild all of its state from reconnects, so the
  relay stays **memory-only, with no persistence**. `boot_id` (fresh on each daemon start) lets the
  client tell a daemon blip from a daemon restart and say "your apps were closed" instead of
  silently showing an empty desktop.

  **Frozen handshake (`relay_wire` v1).** The relay parses only register/registered, join,
  `join_accepted`/`join_denied`/`waiting`, `peer_connected`/`peer_away`/`peer_back`/`peer_left`,
  `peer_accept`/`peer_reject`, ping/pong/leave. Everything else is forwarded unread. Unknown
  fields are ignored and unknown types forwarded; new relay features are advertised in `caps[]`.
  Client and daemon negotiate their own version end to end with a `hello{proto}` through the
  pipe. The relay stops depending on the full `RelayMsg` enum.

  **Policy lives in the daemon.** The relay holds each join at `peer_connected` until the
  daemon answers `peer_accept`/`peer_reject` (30 s → reject). The relay itself keeps only a
  per-IP join rate limit; TLS is terminated by Caddy in front of it.

  **Decided by the user this date:**
  1. **Unattended timeout 30 min.** A desktop with no connected viewer is closed after 30
     minutes, and the disconnect screen says so. The daemon owns this number and sends it to the
     relay, so the seat hold and the desktop's own lifetime can never disagree. If they did,
     a freed seat could hand someone else a still-running desktop.
  2. **One-tap takeover.** "Use it here" moves a seat from a `Live` device to the tapping one
     without asking the holder. This does not reopen I17: only a human tap moves a seat, and
     the displaced device shows "opened on another device" and **does not auto-retry**.
  3. **Approval happens on an already-connected device**, not the daemon CLI. This supersedes
     "CLI prompt first" in `TODO.md`'s confirmation-gate item. **Unresolved:** who
     approves when no device is connected (the very first device, or every device after all
     have disconnected). **Resolved the same day:** a `client_key` the daemon already trusts
     gets in without a prompt; an unknown key waits (parked) until some device is connected
     to approve it. Rejected: a pairing code in the daemon log, and trust-on-LAN.

  **Still open:** the hold length when a viewer goes away (45 s `VIEWER_GRACE` or longer while the
  page is hidden, I13), where the relay is hosted (VPS + domain + coturn alongside, which also
  settles the `2026-09-19` TURN question).

  **Step 2 decided and built (`422c0ff`):** `instance_key` is the pool index,
  `WADO_INSTANCE` (rig numbers 1..N, default 1); relay instance id = `<remote_id>:<key>`.
  Same key + same `boot_id` replaces the stale socket (a redial); a different `boot_id` is
  refused, never allowed to steal. `client_key` moved to step 4: stable instance ids plus
  the existing `?instance=` already give stickiness across relay restarts. Also: the relay now
  closes a client's link the moment its daemon's inbox closes, where before the client
  learned only on its next send.

  Rollout order: (1) `relay_wire` split + version/caps → (2) stable keys → (3) parked joins →
  (4) held seats → (5) two-way liveness (15 s ping, 3 misses), daemon backoff cap 30 s → 5 s
  with jitter, and an exact-type `pong` filter → (6) approval gate → (7) rate limit + Caddy
  TLS → deploy once. **All seven built the same day; see the entry above.**

- `2026-09-20` — **The on-screen gamepad ships in two modes, and the uinput one is not the
  default.** Wayland has no gamepad protocol and never has: games read controllers straight from
  `/dev/input` through evdev, below the display server. So a touch pad drawn on a phone has
  exactly one way to reach a game that reads a controller — a real kernel input device, created
  through `/dev/uinput` (`crates/compositor/src/input/gamepad.rs`). The device reports
  `045e:028e` with the exact `xpad` button/axis layout, so SDL's built-in mapping database
  recognises it and no mapping string has to be shipped anywhere.

  **Named consequence: that device is global to the machine.** It is a kernel input device,
  visible to everything that can read `/dev/input`, inside the session and outside it. There is
  no way to scope a uinput device to a Wayland session; the honest answer was to say so in the
  UI rather than imply an isolation that does not exist. It is created lazily on the first
  gamepad event and destroyed with the session, so a session that never opens the pad leaves no
  phantom controller behind.

  The default is the *other* mode — the pad mapped onto keyboard and mouse events, riding the
  input path that already exists. Two reasons, and the second is the one that decided it:
  `/dev/uinput` is `root:uinput 0660` and the daemon's user is not in that group on a fresh
  machine, so the uinput mode is dead until a system-configuration change is made; and the key
  mode is the only one that can drive mouse-look, which is what the right stick is actually for
  in the games this is aimed at. **Unresolved:** per-game rebinding. The key map is fixed
  (WASD / mouse-look / A=Space / shoulders=mouse buttons), which is a default, not a feature.

- `2026-09-20` — **A phone session is always landscape, decided at `reportScreen` rather than at
  fullscreen.** The panel is reported in whichever orientation the phone is held, and a phone at
  rest is held upright, so a session started without thinking about it came out 720×1600 — a
  360 px-wide logical screen for desktop applications that mostly refuse to be that narrow. The
  output size is fixed at Start and cannot be changed afterwards (invariant #8), so this is the
  only moment the choice exists. The long edge becomes the width for a coarse-pointer device
  with a short edge ≤ 820 CSS px; `screen.orientation.lock` already follows the session's own
  aspect, so pinning the phone to landscape on fullscreen falls out of that with no second rule
  to keep in sync. **Not offered:** a portrait override. It would be one checkbox, and nothing
  currently wants it.

- `2026-09-20` — **`xdg_toplevel.set_fullscreen` is granted, always, and it is not maximize.**
  The handler was smithay's empty default, so an application asking to go fullscreen got no
  configure and no `Fullscreen` state and kept its windowed size — which is the first thing
  every game and every video player does on startup. Restore geometry lives in its own map
  rather than sharing `pre_maximize`: the two states nest, and one map loses the windowed
  geometry the moment a maximized window is then fullscreened. The focus ring is suppressed over
  a fullscreen window (a border drawn over the edges of something that owns the output is a
  border drawn into the picture), and `refit_windows` now treats fullscreen the way it treats
  maximized, so a rotation or a scale change re-sizes it instead of leaving it at the old
  output's dimensions. The `wl_output` argument is ignored: a wado session has exactly one
  output by construction.

- `2026-09-19` — **TURN is in: wado will point ICE at a TURN server.** Chosen over teaching
  `wado-relay` to relay media itself, which this document had named as the fallback. The reason
  is evidence, not preference: on this date a connection failed for twelve minutes between two
  machines *on the same WiFi*, because both ends sat behind a VPN — Cloudflare WARP on the host,
  Zscaler on the client — and a VPN makes the NAT symmetric. Measured from one local socket,
  three STUN servers reported three different external ports. A srflx candidate is then a port no
  peer can reach, and nothing in the STUN-only candidate set can carry the media instead. WARP
  additionally blocks the obvious workaround: binding ICE sockets to the LAN interface returns
  `Operation not permitted`, so no code change routes around it.

  Relaying media through `wado-relay` remains the more self-contained design and is not
  withdrawn — it is deferred. A TURN server is a well-specified component that webrtc-rs already
  speaks as a plain `RTCIceServer` entry, so the wado-side change is configuration rather than a
  new media path; the relay-as-TURN work would be a second media plane to write, test and keep
  fast. Ship the one that closes the failure now. **Unresolved:** where it runs and how
  credentials are issued — a long-lived shared secret in the daemon's environment is the obvious
  first cut and is not yet decided.

- `2026-09-19` — **Popup grabs get the focus-target refactor, not a symptom patch.**
  `SeatHandler::KeyboardFocus` moves from `WlSurface` to a wado-owned enum so
  `PopupManager::grab_popup` can be used at all — `grab_popup` requires
  `KeyboardFocus: From<PopupKind>`, and `impl From<PopupKind> for WlSurface` is forbidden by the
  orphan rule. The cheap alternative (`dismiss_popup` on any input landing outside a popup, ~15
  lines) was offered and **rejected**: it dismisses every open popup on any outside input, where
  the protocol dismisses only popups that asked for a grab, and it would have put a behaviour
  regression in front of Chrome, which manages its own menus correctly today.

  **Known incomplete after the refactor:** the pinned smithay revision ships `PopupGrab`,
  `PopupKeyboardGrab` and `PopupPointerGrab` and **no `PopupTouchGrab`**. wado is touch-first, so
  touch dismissal has no upstream API at this revision and must be written here on top of the
  refactor. Do not record popup grabs as done when the pointer path works.

- `2026-09-19` — **`DEFAULT_RELAY` stays a quick-tunnel URL; the guard is a warning, not a
  stable hostname.** A stale compiled-in default stranded a phone twice, and the failure has no
  trace on either side because the request never reaches the relay. Two changes make that
  acceptable without buying a domain: `scripts/rig.sh` now **reuses** a live cloudflared instead
  of minting a new URL on every start, so the default goes stale far less often; and it warns
  loudly whenever the deployed default is not the live tunnel. Pasting the URL remains the
  workflow. Revisit if the relay ever gets a host of its own — which is also where TURN would
  live.

- `2026-09-14` — **A Remote ID names a pool of daemons, not one daemon.** Multi-device lands on
  the *supervisor* path rather than as an in-process refactor. Several `wado` processes register
  under one Remote ID; the relay gives each joining client one of its own. Each is a whole
  process with its own compositor, encoder, Wayland socket and applications.

  The in-process alternative — N `Wado` states on N threads — was costed and rejected. The
  server holds **one** `TrackLocalStaticSample`, `build()` sets `WAYLAND_DISPLAY` with a
  process-global `set_var`, and `headless.rs` + `state.rs` (78 KB) are written against a single
  `session_active`. The pool moves all of that to the OS for free and buys the crash isolation
  that made the child-process supervisor a milestone in the first place: a segfault in one
  device's graphics stack cannot reach another's. It also grows without a restart — verified
  2→4 daemons while two sessions were streaming.

  `registry` and `rooms` are keyed by an **instance id** minted per registration.
  `crates/relay/src/room.rs`'s "one viewer at a time (iter 1)" is superseded. Pool size is
  `WADO_INSTANCES` (default 2) — a fixed ceiling, deliberately, so the limit is one number to
  read; spawn-on-demand is the upgrade path. The real ceiling is concurrent hardware encode
  sessions, which is **not yet measured**.

- `2026-09-14` — **Withdrawn the same day: last-writer-wins room displacement.** A room was tied
  to a client's socket, so a tab left open with its session already stopped refused every later
  device — 14+ refusals over four minutes, while closing the tab let the next device in in one
  second. Displacement looked like the fix and instead produced a steal-war: every evicted
  client reconnects and evicts the other. **132 joins in two minutes.**

  This is `issues.md` **I17**, already recorded at an 18 s period, and the client already
  guarded it — `OCCUPIED_RE` in `js/relay_link.js` matches the relay's denial wording and backs
  off hard. Displacement bypassed that guard by never denying. *A fix that removes the condition
  a guard tests removes the guard.*

  The rule now: **nothing takes a room from a live client**, not even the same device reclaiming
  its own instance (two tabs of one browser would recreate the war). Contention is answered with
  a different daemon, or a refusal carrying the occupancy and how to raise the limit. The phrase
  "already has an active connection" in `signaling.rs` is load-bearing and pinned by
  `scripts/relay-link-check.mjs`.

- `2026-09-12` — **A dead peer connection no longer ends the session; `viewer_watchdog` is the
  only teardown.** `on_peer_connection_state_change` stopped the compositor the instant WebRTC
  reached `Failed` or `Closed`. On a phone that is a cell handoff, a lift, or a tunnel — and each
  one cost the viewer every window, every application and a cold Chrome launch. Measured on the
  roaming run: 17 Chrome launches in one daemon run; `rtt` spiked to 2191 ms at 17:57:32 and
  `SESSION stopped` was logged at 17:57:33.

  The `Failed`/`Closed` arm is now informational. `viewer_watchdog` already implemented the
  correct rule — `VIEWER_GRACE` (45 s) of relay-link silence **and** a non-connected peer
  connection — and the immediate path was pre-empting it. The watchdog is spawned once, outside
  the relay reconnect loop, over the same `session_started`/`last_relay_msg` `Arc`s for the
  daemon's lifetime, so a daemon-side relay reconnect does not strand it.

  **What this changes:** session lifetime is now decoupled from transport lifetime. A viewer that
  re-offers through the still-open relay WebSocket keeps its session. Two consequences are
  recorded in `issues.md` rather than fixed: a tab that gave up retrying pins the session until
  `pagehide` (I12), and 45 s is now load-bearing and probably too short for real dead zones (I13
  — the number is the user's call, not a code decision).

  The client half landed with it and is not optional: `W.handleFailure` and `W.resync` called
  `connectWebRTC()`, which POSTs to `/offer` — **an endpoint that does not exist in relay mode**.
  Every relay-mode recovery had been spending its retries on a fetch that could not succeed. The
  relay re-offer (`W._relayNegotiate`) already existed and nothing called it. `W.reconnectWebRTC`
  is the one-line chooser; `scripts/reconnect-check.mjs` pins the branch. Server-only would have
  been worse than the bug: a surviving session the client cannot re-offer to.

- `2026-09-12` — **Fractional scale: `Scale::Custom`, and the integer half is `floor`, not
  `round`.** `headless.rs` used to round the requested scale before handing it to *both*
  consumers, so a session asked for 1.25 was given 1.0 on a protocol wado already implements
  (`requested=1.25 applied=1.0` in the log). Root cause from history: `634c603` added the
  rounding when `wp_fractional_scale_v1` did not exist, and `8e9f3e7` implemented the protocol
  and re-enabled the fractional UI options *on top of* the rounding instead of removing it.
  Fixed in `8f363e6` with `Scale::Custom { advertised_integer, fractional }` — the fractional
  half is now exact for anything that speaks the protocol (Chrome, modern GTK).

  `advertised_integer` was `round()` in that commit and is `floor()` now. The reasoning that
  picked `round` was "wrong in the safe direction", and it was wrong: `round == ceil` at 1.5,
  1.75, 2.5 and 2.75, so it left the *original* clipping bug fully intact at every scale above
  1.25 — including the 1.75 in live use. Live testing could not have caught this, because every
  client tested was Chrome and Chrome binds `wp_fractional_scale_v1`; the legacy branch was
  never exercised. `floor` inverts the failure: a legacy client is told 1, draws 1x and is
  composited soft rather than told 2, drawing 2x and overhanging its own area. Through an H.264
  stream, soft is nearly free and clipped chrome is broken.

  Confirmed protocol-event-only before changing it: `integer_scale()` has no consumer anywhere
  in `crates/` — the render and capture paths size from the output *mode*, not the scale — so
  this affects the `wl_output.scale` event and nothing else. `new_fractional_scale` now logs
  each surface that takes the fractional path, so the population the integer branch actually
  serves stops being invisible.

- `2026-09-12` — **`zwp_pointer_gestures_v1` (`198faca`, `1dd7c6f`): two-finger pinch and
  rotate.** Only the *pinch* half is driven; swipe and hold are advertised but never emitted,
  because the client's two-contact FSM already spends a two-finger drag on the scroll axis.
  Three things worth keeping: (1) the pinch rides the **same** gesture as the scroll on
  purpose — that is what libinput emits for a touchpad and what toolkits are written against —
  so `GesturePinchUpdateEvent.delta` is sent as **zero**, or a toolkit pans twice for one drag.
  (2) `scale` is absolute against the begin distance, `rotation` is a delta in degrees since the
  previous event; swapping them silently inverts a zoom. (3) The open/closed state is tracked on
  the **compositor** side (`Wado::pinch_open`), not the client, because the client is the half
  that can vanish — a disconnect or `resetInput` nulls the gesture with no end sent, and an
  un-ended pinch leaves the toolkit stuck in zoom mode. Windows outlive sessions here, so an
  orphan would survive into the next one.

- `2026-09-12` — **The v0.0.2 jitter-buffer reclaim is retracted; `jitterBufferTarget` is a
  floor, not a knob.** `reassertPlayout` re-set the hint whenever the buffer ran past it and the
  RTT looked healthy. It fired 15 times in one session while the buffer drained
  56→55→54→54→53→52 — its natural rate, no inflection at any reassert. Playout is roughly
  `max(target, browser timing model)`, so the hint can raise the delay and can never lower one
  the model is driving; two traces show `jtarget` tracking `jbuf` (26/24 desktop, 23/31 phone)
  while the hint reads back as the 20 we set. Deleted in `0f66d04`, with the reasoning left in
  `webrtc.js` so it is not retried. The one-time floor at connect is kept — that half works. The
  underlying behaviour is still real and still unfixed: a hitch permanently inflates the buffer
  by roughly +5 ms and no playout-hint approach can reclaim it.

- `2026-09-12` — **Withdrawn: "decode cost explains why 120 fps drops frames."** Recorded in
  `plan/optimisation.md` R1/O6 and now removed. The comparison behind it was 60 fps at
  12000 kbps/Veryfast/scale 1.75 against 90 fps at 5676/Ultrafast/scale 2.0 — three variables
  besides the one under test. Matched against the correct 60 fps trace, `dec` is very nearly
  **proportional to bits per frame** (0.667 vs 0.69), not a fixed cost of resolution, so at a
  fixed bitrate a higher frame rate makes decoding *cheaper* per frame. R1 needs a new cause.
  Rule written down as a result: never compare `dec` or `kbps` across two sessions unless the
  `compositor session active` lines are identical except for the single variable under test.
  This is the fifth time a config confound has produced a wrong conclusion here.

- `2026-09-11` — **Client UX track implemented (M-C1…M-C4). Four deviations from the design,
  and three bugs the work exposed.** Built in milestone order, one commit each
  (`dd1ba02`, `169e843`, `afb7359`, `ea7ccfc`). The design entry below still holds; this
  records only where reality argued back.

  **Deviations.** (1) *Appearance became its own settings group* rather than living under
  Live — a theme picker reads oddly beside "launch a command", and the paste box is tall
  enough to bury whatever sits next to it. (2) *The bundled base16 palettes live in
  `js/theme.js`, not Rust.* The saved scheme has to be applied during the bridge's synchronous
  head or the page renders in the default palette first and visibly repaints; Rust still owns
  parsing, which was the part the design called pure data. (3) *The control bar landed in
  M-C2, not M-C3* — the mobile sheet needs something to open it, and a throwaway toggle would
  have meant writing the bar twice; M-C3 only added four buttons to it. (4) *The relay does not
  consolidate onto one control message* — recorded in full in the design entry below.

  **Bugs this exposed, none of them in the plan.** (a) `use_effect` runs before the bridge's
  async settings load, so the persist effect wrote the defaults over the saved blob on every
  page load: **nothing would ever have survived a reload.** Gated on a `loaded` flag. (b)
  `placement.rs` implemented `Placement::Maximized` by setting the window's size but never
  `xdg_toplevel::State::Maximized`, so apps were resized without being told they were
  maximized — they draw differently when they know (GTK header bars), and the new maximize
  toggle had nothing to read back, so its first press on a start-maximized window would have
  maximized it again. (c) dx's generated `index.html` carries a viewport meta **without**
  `viewport-fit=cover`, which silently resolves every `env(safe-area-inset-*)` to zero; a
  custom `crates/client/index.html` now supplies it.

  **On the two latency bugs the design session found by reading:** both are fixed, and the
  consequence is worth stating plainly rather than filing as done. `js/relay.js` never started
  the latency probes, so **every relay-mode latency breakdown before this commit was absent,
  not good** — an empty readout, which is easy to misread as a healthy one. And `js/webrtc.js`
  started them unconditionally, so "Debug off" still put a 2 Hz ping on the input data channel.

  **Verification, stated honestly.** Workspace builds, `dx build` produces a WASM bundle, 15
  unit tests pass, and the two new HTTP routes were exercised against a running server (71
  applications discovered with `Exec=` field codes stripped; unknown window verb → 400 naming
  the valid four; a valid verb with no session → 200 plus the compositor's warning rather than
  silence; the removed `/session/launch` → 404). **No part of this has been opened in a browser
  or touched on a phone**, which is where all four milestone thresholds actually live. The
  unverified surfaces are enumerated in `TODO.md`'s Handover, worst-first.

- `2026-09-11` — **Client UX milestone set (M-C1…M-C4): restructure, mobile, window control,
  app launcher. Design settled in a grilling session; no code written.**
  The ask was "better UI, organised settings, a universal debug toggle", which on inspection
  is four separable bodies of work — two of which reach the compositor. Decisions, in the
  order they were resolved:

  **Debug is a registry, not four hand-wired toggles.** Each debug item is declared once as
  `{ id, label, default, js: Option<&str> }`; one generic checkbox handler renders it,
  persists it, and — when `js` is `Some` — evals `window.__wado.<setter>(bool)`. A master
  switch gates the group. `fps`/`ping` are pure Rust view gates (`js: None`); `touches` and
  `latency` carry setters. Adding a debug view becomes one table row plus one JS setter
  instead of edits in four places. **Master defaults ON with today's per-item defaults
  (fps/ping on, touches/latency off)**, so nothing visible regresses; OFF is an explicit
  clean-screen action. **The server keeps timing unconditionally** — `timing.rs` is
  `Instant::now()` deltas per frame, so gating it would buy nanoseconds for protocol churn.
  **Two bugs this exposes and fixes:** `js/webrtc.js:35` calls `W.latency.start(pc)`
  *unconditionally*, so with the Debug toggle off the client still runs a 2 Hz ping down the
  input data channel and a 1 Hz `fetch /timing` — traffic on the channel invariant #1 exists
  to protect, for numbers nobody displays. And `js/relay.js:168` starts stats but never
  starts latency, so relay mode has no breakdown at all. The toggle was wired to the display
  and not to the work.

  **Settings group by *when they apply*, not by subsystem:** Connection / Session (locked
  while running) / Live (instant) / Debug. The code already admitted the old grouping was
  accidental — `"(applies at Start)"` is hand-appended to four separate labels. The group
  header says it once, and Session inputs can go `disabled` mid-session so the rule is
  enforced rather than documented. Subsystem grouping was rejected precisely because it
  scatters the restart-required items across three groups.

  **Everything persists in one `localStorage` key** (`wado.settings`, one JSON blob, written
  on change, restored on mount, unknown keys ignored on read so an old blob never breaks a
  new build). Nothing persists today — verified, zero `localStorage` references in the crate
  — so a phone user retypes relay URL, Remote ID and resolution every session. Storage lives
  in the JS bridge like every other browser-only concern, not behind a new `web-sys` dep.

  **One settings component, two presentations.** Docked side column ≥720px, slide-over sheet
  below; same rsx, CSS only. `main.css` today is 67 lines with zero media queries and a fixed
  `flex: 0 0 320px` panel — on the 1080×2400 phone this project exists for, that eats a third
  of the screen. Mobile scope: bottom sheet, `env(safe-area-inset-*)`, `100vh` → `100dvh`
  (the URL bar makes `100vh` wrong by ~60px), 44px touch targets, Fullscreen API,
  `screen.orientation.lock()`, `navigator.wakeLock`, `overscroll-behavior: none` so a drag
  gesture cannot pull-to-refresh the page and kill the session. **On-screen-keyboard handling
  (VirtualKeyboard / visualViewport) is deliberately deferred** to its own pass — browser
  behaviour is too inconsistent to bundle.

  **Remote window control is a new feature, not a UI change.** Verified absent:
  `CompositorCommand` is exactly `Start | Stop | Launch | ForceKeyframe` (`control.rs:29`),
  and `handlers/xdg_shell.rs` has no `maximize_request`/`minimize_request`/`fullscreen_request`
  handler and nothing tracks a window list. Scope: **act on the focused window only** —
  maximize/restore, minimize, close, cycle-focus — because the compositor already knows what
  is focused, so no window list has to be tracked, serialised, or pushed over *both*
  transports. **Trap caught during design: with no window list, an unmapped window is
  unreachable forever.** So **minimize means lower-to-back**, not unmap: cycle-focus always
  reaches it and it is always recoverable, with nothing extra to track. **Named ceiling, not a free win:** with no titles,
  `CycleFocus` is a blind rotation — the user taps until the right window appears, and each
  tap costs a full re-render round trip to see what they got. That is acceptable at two or
  three windows and degrades from there. A real task-switcher with titles and per-window
  restore is the upgrade path, deferred to its own milestone.

  **All control commands collapse onto one generic route — on the HTTP side only.**
  `POST /session/control` taking a typed `SessionControl` enum (`Launch { command }` |
  `Window(WindowAction)`). **Corrected while implementing:** the relay does *not* consolidate.
  `RelayMsg` is a flat, `#[serde(tag = "type")]` namespace that the relay forwards verbatim,
  where one variant per action is already the idiom and costs zero per-action plumbing — so
  `SessionLaunch` stays and `SessionWindow` joins it as a peer, and nesting would only have
  produced two different shapes for one layer. The `/session/launch` *route* is removed; the
  relay message of that name is not. Today every command is hand-plumbed into *two* places —
  `POST /session/launch` in `lifecycle.js:62` and a `session_launch` WS message in
  `relay.js:223` — so four new commands would mean eight new plumbing sites. Control plane
  over HTTP/WS is explicitly permitted by invariant #2. **`/session/launch` is removed, not
  aliased:** client and server ship from one repo in one deploy and the client is a WASM
  bundle that reloads, so there is no third-party consumer and no version skew to protect
  against — a deprecated alias would be dead flexibility for a compatibility problem that
  does not exist. Sending window commands down the WebRTC input data channel was rejected: lower latency, but it puts non-input traffic on the
  channel invariant #1 protects, and it only works once WebRTC is up.

  **Window controls sit on an auto-hiding edge bar** over the video (fades after ~2 s,
  returns on tap-near-edge), not in the settings sheet. Reasoning: these replace compositor
  keyboard shortcuts, and a shortcut that costs open-sheet → tap → dismiss is not a shortcut.
  The bar also carries the ⚙ that opens settings. Gesture-only control was rejected as
  undiscoverable and as a collision with the touch events the session itself needs.

  **The launcher reads XDG desktop entries.** Server scans `/usr/share/applications` and
  `~/.local/share/applications`, serves `GET /apps` (`name` + `exec`, `%f`/`%u` field codes
  stripped); the client filters as you type, with the free-text box kept as an escape hatch.
  A `.desktop` file is an INI — reading `Name=`/`Exec=` is ~30 lines, so no new dependency.
  Relay mode needs an `apps` request/response message; it has no HTTP path to the server.
  **Security note, not acted on yet:** `CLAUDE.md` records that the control plane binds
  localhost *because* the launch command is free-form RCE. A picker is the mechanism that
  could later shrink that surface to a known set of installed apps — worth revisiting when
  the auth gate lands and the bind widens.

  **`main.rs` splits** (673 lines, one `App()`, ~30 signals, the bridge pump and all rsx) into
  `state.rs` (signals + storage), `bridge.rs` (BRIDGE_JS + event pump), `cfg.rs` (signals →
  `SessionConfig`), and `ui/{connection,session,live,debug,stage}.rs`, with `main.rs` a thin
  dispatcher. CSS splits three ways — `base` (base16 tokens, reset, controls), `layout` (shell,
  breakpoint, sheet, safe areas), `stage` (video, stagebar, badges, latency chips, logs, edge
  bar) — deliberately *not* one file per UI module, which would duplicate the responsive rules
  across six files. **Noted tension, resolved explicitly:** the session ran under a
  laziness/YAGNI directive that wants the fewest files, against `CLAUDE.md`'s "atomic module
  separation outranks brevity". Resolution: the project rule wins on *structure* (split the
  file), the laziness rule wins on *scope* (no settings-schema framework, no trait with one
  impl, `localStorage` stays ~20 lines and not a persistence layer).

  **Look: minimal, hand-written, themed with base16.** No CSS framework and no new build
  step — Tailwind was rejected as a dependency plus a `dx` pipeline change for CSS that fits
  in ~200 lines. The token layer is the **base16 scheme** (`--base00`…`--base0F` on `:root`),
  which costs nothing but naming and buys a principled home for colours that are currently
  hand-picked hexes: `base08` red → ERROR lines, Stop, `badge-bad`; `base09` orange → the
  software-encoding banner; `base0A` yellow → WARN, `badge-warn`; `base0B` green → Start,
  `badge-good`, accent; `base0D` blue → Launch and links; `base00/01/02/03/05/07` → ground,
  surface, border, dim text, text, bright. Ship 3–4 schemes with a picker in Settings
  (persisted in the same blob) **plus a paste box** that accepts a scheme's 16 hex values, so
  any of the hundreds of published base16 schemes works without a rebuild. Parsing is a scan
  for `base0X`-to-hex pairs — it handles both the YAML form schemes are distributed in and a
  bare list of 16 values, in ~10 lines and no YAML dependency; it lives in Rust (`state.rs`)
  because it is pure data with no browser API involved. **One forced override:** the stage
  backdrop behind the video stays dark regardless of scheme. base16 includes light schemes,
  and light chrome bleeds onto the picture — the theme may not wash out the thing being
  streamed.

  **Motion is budgeted, because this client is decoding 60–120 fps while it animates.**
  Everything that moves is `transform`/`opacity` only — compositor-thread, never layout or
  paint — on one shared easing/duration token pair. Exactly six: sheet slide-in (220 ms),
  edge-bar fade (180 ms), settings group expand (180 ms), button press (90 ms), status/banner
  fade (180 ms), launcher list filter (120 ms). `@media (prefers-reduced-motion: reduce)`
  disables all of it. **Explicitly never animated: the telemetry values.** `main.css` already
  carries the reason on `.latval` — fixed-width tabular figures exist so the numbers do not
  jitter horizontally, because the readout's whole job is spotting which stage grew; a
  count-up transition would destroy exactly that. Video, badges and the encoder banner are
  likewise static. **`backdrop-filter` glass was considered and rejected** — it forces a
  readback of the composited video underneath it every frame, which is the one effect this
  client genuinely cannot afford.

  **Unconditional, and not gated by any of this:** the software-encoding banner and the
  `badge-bad` pipeline badge stay outside the debug master switch. Invariant #5 is a
  user-facing warning, not a debug view, and it keeps its server-log half.

  **Order — client first, each confirmed in a browser before the next starts** (`CLAUDE.md`:
  one module at a time, in milestone order): **M-C1** restructure → **M-C2** mobile →
  **M-C3** window control → **M-C4** app launcher. Client-first because M-C1 creates the
  structure the later UI hangs off; doing window controls first means writing them into the
  673-line file and moving them twice.

- `2026-09-11` — **Damage tracking measured: currently inert, and CBR is why it matters.**
  The `OutputDamageTracker` has been wired since M0 but is passed `age: 0`, which sends
  smithay down the "no old damage available, re-render everything" branch unconditionally,
  and `render_output`'s `RenderOutputResult` is discarded — so neither partial redraw nor
  the no-damage early-out can ever fire. Every tick does a full capture + full encode +
  send regardless of what changed. Measured at 1280x720/120, `vaapi-dmabuf`, release,
  Radeon 880M: an empty desktop reports **241/241 frames with zero damage** and still
  transmits 2.00 Mbps; an idle terminal with a blinking cursor reports 228/232 (98.3%) at
  1.94 Mbps. Session cost vs. the no-session floor: CPU 13.0% of a core → 0.0%, GPU busy
  13% → 8%.
  **The non-obvious part:** `avg_frame_bytes` is 2095 — constant to the byte across all
  three scenarios, and 2 Mbps ÷ 8 ÷ 120 = 2083. The CBR lock from invariant #7 makes the
  encoder *stuff* frames that have nothing to encode, so the usual reassurance that "H.264
  already makes static content nearly free via skip macroblocks" does **not** apply here.
  While CBR holds, damage-gating is the only mechanism that makes a static screen cheap.
  **Not done, deliberately queued** behind the jitter work (see the entry below): skipping
  produces irregular frame intervals, which is precisely the load the receiver's playout
  buffer absorbs, and that buffer is the current prime suspect for the 120 fps judder.
  Adding a second source of irregularity before the `W.playoutMs` sweep is settled would
  confound the diagnosis. Tracked in `TODO.md` under GOOD-TO-HAVE with the full numbers.
  Scope when it happens: `age: 1` plus honouring the result, a ~1 fps heartbeat floor so a
  gap is not read as a stall, an IDR on resume and for any newly-joined viewer, and a check
  that the capture target genuinely never swaps buffers — `age: 1` is a lie the moment it
  does, and the symptom is stale-content artefacts. `age: 1` on its own is worthless:
  measured capture 0.15 ms → 0.016 ms with encode unchanged at 3.4 ms, because the cost is
  all in the encode. Bounds on the eventual win: ~98–100% of bandwidth/CPU/encode-GPU on
  idle content, **zero** on full-motion content, and **zero for latency** — dragging a
  window damages every frame, so the interactive path is the one case it never touches.

- `2026-09-11` — **Docs split three ways: compass / state / plan. `TODOS.md` retired.**
  `CLAUDE.md` had grown to 286 lines carrying a command cookbook, a UI walkthrough and a long
  "Current status" narrative — and some of that narrative described **code that was never
  written** (an `event.rs`/`SessionEvent`/`website/events.rs` SSE chain; see the latency entry
  below). Stale instructions are worse than none, so it is now a **compass only**: mission,
  the eight hard invariants, architecture orientation, the version pins *and why they exist*,
  a hazards list, licensing, intent guards. 161 lines, no commands, no examples, no status.
  Build commands were dropped deliberately — they are discoverable from the workspace — while
  the non-obvious traps that are NOT discoverable were kept and sharpened (GPU needs the
  sandbox off; never loose-match when killing the process; dev shell vs host library worlds;
  `cargo fmt` is dirty at baseline so repo-wide formatting buries real diffs; unsynchronised
  browser/server clocks must not be summed; panics are contained, segfaults are not).
  **New `TODO.md`** is state, not architecture: a **progress dashboard** (12 done / 44 open /
  56 total, counted mechanically from the checkboxes so it can be re-derived rather than
  trusted — the first hand-count was wrong by one), categories by urgency
  (NOW / NECESSARY / GOOD-TO-HAVE / DEFERRED / **HYGIENE**, the last being new), and a
  **Handover** section at the top holding exactly what must survive a session boundary:
  the seven browser-only verifications nobody has done, the two known-unknowns worth one
  measurement each, and the two open decisions needing a human (smithay manifest pin vs
  migration; relay TLS approach). `TODOS.md` was deleted after migrating all 40 of its items
  (two pairs merged: the connect-approval workflow into the confirmation gate, and the two
  multi-relay items into one) plus five new HYGIENE entries — no tests anywhere, the 21 KB
  four-job `website/mod.rs`, the `cargo fmt` baseline, relay mode lacking a timing endpoint,
  and a docs-vs-code audit. Its relay target-state diagram, iteration table and implementation
  notes moved to the end of this file, where architecture belongs. `.gitignore` follows the
  existing convention for these local docs (`/TODOS.md` → `/TODO.md`); flip that if the
  tracker should be committed instead.

- `2026-09-11` — **Remaining latency fixes applied + per-stage latency breakdown (Debug toggle).**
  Follow-up to the jitter entry below, after the user reported still-jittery input and
  "painfully slow" window dragging.
  **Root cause of the drag lag (confirmed):** `input_pointer.js` and `input_touch.js` sent
  `window_drag` motion on **every** `pointermove`, uncoalesced, while plain hover motion right
  beside it *was* rAF-coalesced. A 125–1000 Hz mouse therefore pushed up to ~1000 JSON messages/s
  into a reliable+ordered SCTP channel; the send buffer backed up and every later event queued
  behind it, so a drag fell further behind the longer it lasted. The one path that most needed
  rate-limiting was the only one without it.
  **Changes:** (1) new `js/input_coalesce.js` — one coalescer for ALL positional events
  (pointer motion, window-drag motion, per-contact touch motion), newest-wins, one send per
  animation frame, with `now()` for terminal events that flushes the queue first so a release can
  never overtake a pending motion and snap the window back. (2) **Two input data channels**
  (invariant #1, as WADO_PLAN specified): `INPUT_CHANNEL` stays reliable+ordered for anything
  unrecoverable if lost (buttons, keys, scroll, touch, drag down/up), new `MOTION_CHANNEL`
  (`wado-motion`) is unordered + `maxRetransmits: 0` for pointer/drag **motion** only — absolute,
  latest-wins payloads the compositor already tolerates out of order (`WindowDrag::Motion` is a
  no-op when no move is in progress). Touch motion deliberately stays reliable: a finger generates
  events at a fraction of a mouse's rate so it was never the saturation source, and a dropped
  `wl_touch` motion on a live slot is messier. (3) `FRAME_CHANNEL_CAPACITY` 4 → **2** (every queued
  frame is latency the viewer eventually sees). (4) `ChannelSink` now stamps each sample with
  **real elapsed time** since the last accepted hand-off instead of a nominal 1/fps, so the RTP
  clock tracks wall clock across drops/stalls rather than letting the receiver compensate by
  growing its playout buffer; `FrameMsg` became a struct (`data`/`duration`/`queued_at`).
  **Latency breakdown (new):** `crates/compositor/src/timing.rs` (single-job) times **capture** and
  **encode** separately inside each render tick and publishes rolling means over a
  `tokio::sync::watch` (latest-wins, no backlog) created in `build()` so it survives session
  cycles; the **queue** leg is measured by the server's pump (the only side that knows when a frame
  was picked up) as an EWMA off `FrameMsg::queued_at`. Served by a new **`GET /timing`** JSON
  endpoint. Client `js/latency.js` (single-job) merges that with its own `getStats()` legs
  (`net` = RTT/2, `buf` = playout depth, `decode` = totalDecodeTime/framesDecoded) plus an
  **input round trip** from a new `InputEvent::Ping` the server echoes as `{"t":"pong"}` directly on
  the data channel (never forwarded to the compositor, so the number measures the input path, not
  the render loop). Debug → **"Latency breakdown (per stage)"** renders it as chips in pipeline
  order. **Deliberately no single glass-to-glass total:** browser and server share no clock, and
  `input` is a round trip while the others are one-way, so summing them would manufacture a
  confident meaningless number.
  **Measured (1280x720@60, `vaapi-dmabuf`, debug build):** capture **0.19–0.39 ms**, encode
  **2.6–3.1 ms**, queue **0.05–0.12 ms**, tick 16.67 ms, **60.0 fps**, **0 drops**. The server half
  of the pipeline is ~3 ms, i.e. it is NOT where the remaining perceived latency lives — that
  points at the browser playout buffer and the network legs, which is exactly what the new client
  rows are there to show.
  **Also found, NOT fixed (doc/reality mismatch):** CLAUDE.md's "Current status" describes a live
  SSE downgrade marker wired via `event.rs::SessionEvent::EncoderChanged` → `event_tx` →
  `website/events.rs` → a named `event: encoder` SSE arm → `logs.js` listener. **None of that
  exists**: there is no `SessionEvent`, no `event.rs`, no `website/events.rs`, and `/events` is a
  hand-rolled raw-TCP stream carrying only log lines (`logs.js` has a single `onmessage` →
  `{type:"log"}`). The client's `"encoder"` handler is fed by the `/session/start` reply instead.
  That is why `/timing` is a polled endpoint rather than an SSE event. `website/mod.rs` is also
  21 KB doing many jobs (routing + WebRTC + SSE + pump) and wants the split CLAUDE.md already
  implies — both left as separate work.
  **Verified:** `cargo build`/`cargo test`/`cargo check -p wado-client`/`dx build` green; 3x
  start→launch→stop cycles survive in one process (3 active / 3 stopped, 9 healthy pacing windows,
  no panics, 0 drops); `/timing` returns live numbers and keeps working across a session restart.
  **Not verified (needs a browser):** the coalescer's effect on drag feel, the two-channel split
  under real input, and every client-measured row.

- `2026-09-11` — **Input/output jitter + constant lag: root-caused and fixed (input flush, timer
  clamp, receiver playout).** Symptom: "input output is jittery and even at 0 ms a little lag can
  be noticed."
  **Cause 1 (confirmed, the jitter) — remote input was never flushed to Wayland until the next
  render tick.** `flush_clients()` had exactly ONE call site: the tail of `render_tick`
  (`headless.rs`). `Wado::handle_remote_input` only queues synthesized pointer/key events into each
  client's outgoing buffer, so every remote input waited for the next tick to even reach the app —
  and the app's response waited for the tick after that. Quantised to the frame period with a
  variable phase; that IS the jitter. Fix: `Wado::flush_clients()` (new pub method on the state, so
  the server still names no Smithay type) called from the event loop's **post-dispatch callback**
  (`crates/server/src/main.rs`) — once per loop iteration however many sources fired, and it covers
  commands and Wayland traffic too.
  **Cause 2 (latching, fixed defensively) — render-timer deadline accumulation.** `headless.rs` did
  `ToInstant(deadline + frame_period)` where `deadline` is the *scheduled* instant. One overrunning
  tick puts every later deadline in the past → timer permanently ready → calloop never idles →
  input/Wayland/control starve behind a busy render loop, and it never recovers. Now clamped: keep
  the original phase when on time (drift-free), re-phase from `now` when behind (restores the idle
  gap, drops the missed frames).
  **Cause 3 (the constant lag) — no receiver-side playout control.** `webrtc.js` never set
  `jitterBufferTarget`/`playoutDelayHint`, so the browser's adaptive video jitter buffer (~100 ms+,
  and wandering) applied. It is invisible to `currentRoundTripTime`, which is exactly why the
  stagebar could read 0 ms while the picture trailed. New `W.minimizePlayoutDelay(receiver)` sets
  both knobs (standard + legacy Chrome spelling) on track arrival.
  **New telemetry:** `crates/compositor/src/pacing.rs` (single-job module) reports achieved-vs-target
  fps and max render-tick interval per ~2 s window — `debug` when healthy, `warn` only on sustained
  shortfall (gate = mean interval > 1.25x budget; an earlier naive `dt > budget` gate cried wolf on
  a steady 60 fps loop). `sink/channel.rs` now counts dropped frames instead of swallowing the
  `try_send` error. Client stagebar gained a **"N ms buf"** readout beside ping (playout depth).
  **Measured (this box, 1280x720@60, debug build):** `vaapi-dmabuf` holds **60.0 fps, mean interval
  16.7 ms, max 16.7 ms** — the render loop is NOT a jitter source on the hardware path. `x264-cpu`
  manages only **13.6 fps, mean 73.4 ms** (debug build, unoptimised CPU readback/convert), i.e. the
  behind-budget path Cause 2 protects. **Frame drops: 0 on both.**
  **Consequently deferred, now with evidence rather than suspicion:** (a) the fixed
  `Sample.duration = 1/fps` RTP timestamp is NOT live — drops are 0, so the RTP clock is not
  slipping; (b) `FRAME_CHANNEL_CAPACITY = 4` adds no standing latency — the queue never fills;
  (c) rAF coalescing of pointer motion (`input_pointer.js`) costs up to one display frame — real but
  small, revisit only if jitter persists.
  **Still open, deliberately NOT bundled here:** the input data channel is `ordered: true` + fully
  reliable for ALL input (`webrtc.js`), which violates invariant #1 / the WADO_PLAN design
  (position/motion should be unreliable-sequenced, buttons/keys reliable). It is provably not this
  bug — reliable and unreliable are identical on a loss-free 0 ms link — but it will cause
  head-of-line-blocked input on a real lossy network. Own change, own entry.
  **Hardening after review:** `pacing.rs` reports on a full tick window **or** 2 s elapsed,
  whichever comes first — windowing on tick count alone made the telemetry quieter the worse the
  stall got (at 13.6 fps one line per ~9 s; a fully stalled loop would never report, and silence is
  indistinguishable from "no session"). Also regression-tested the now-**unconditional** flush: the
  old code only flushed inside `render_tick`, which early-returns when `!session_active`, so the
  post-dispatch flush newly runs during idle/teardown. 3x start→launch→stop cycles in one process:
  server survives, 3 sessions active / 3 stopped, 9 healthy pacing windows, no panics, 0 drops.
  **Verification honesty:** cause 1 is confirmed by code (single `flush_clients` call site) and
  cause 3 by mechanism; **cause 2's clamp is correct-by-construction, NOT A/B-measured** — both
  measured runs are post-fix, and healthy hardware pacing looks identical under the old code.
  **Not yet user-verified:** the browser-side effect. Needs one `dx serve` session with Debug→ping
  on to read the new "ms buf" number — under ~30 ms means cause 3 is fixed; still 100 ms+ means the
  hint did not take (only Chromium implements `jitterBufferTarget`; Firefox has neither knob) and
  the remaining lag is elsewhere.

- `2026-09-11` — **`wasm-bindgen` pin bumped `=0.2.118` → `=0.2.126` (forced by dioxus-cli 0.7.10).**
  Fallout from the flake update below: `dx build` failed with *"Incorrect wasm-bindgen-cli version:
  project requires version 0.2.118 but version 0.2.126 is installed"*. `dx` brings its own
  wasm-bindgen-**cli** and demands an EXACT match with the `wasm-bindgen` **crate**; dioxus-cli
  0.7.9 shipped 0.2.118, 0.7.10 ships 0.2.126. Bumped the exact pin in `crates/client/Cargo.toml`
  (pulling web-sys 0.3.103 / js-sys 0.3.103 / wasm-bindgen-futures 0.4.76). The old comment on that
  pin predicted exactly this ("will have to change when I update to the next version of dioxus") and
  is replaced with the lockstep rule. **Treat dioxus-cli and this pin as one version, bumped
  together** — CLAUDE.md's "Run client" bullet now carries that rule. Verified:
  `cargo check -p wado-client` green, **`dx build -p wado-client` green** (wasm-bindgen step passes,
  bundle written to `target/dx/wado-client/debug/web/public`), host `cargo build` + `cargo test`
  still green. Also corrected a stale CLAUDE.md claim: `dx` and the `wasm32-unknown-unknown` target
  are present in the dev shell and the wasm build is no longer a user-side-only step.

- `2026-09-11` — **Flake `nixpkgs` updated to 2026-09-07 (rev `dc5d91f`); `ffmpeg` pinned to
  `ffmpeg_8`.** Superseded the `2026-09-08` decision to hold the flake at 2026-05-23 (rev
  `64c08a7`) — the update was taken on request and **verified**. The flake was ~3.5 months
  behind the system (2026-05-23 vs 2026-08-18), which is *why* shell mesa (26.1.1) and system
  mesa (26.2.0) disagreed in the first place. New shell: mesa **26.2.2**, libva **2.24.1**,
  rustc **1.97.1**, cargo 1.97.0, dioxus-cli **0.7.10**, libdrm 2.4.134. **`ffmpeg` → `ffmpeg_8`
  (8.1.2) in `nativeBuildInputs`** because that nixpkgs moved `pkgs.ffmpeg` to **9.0.1**, and
  `ffmpeg-the-third = "5.0.0"` (crates/compositor) is built against the FFmpeg 8.1 ABI;
  `pkg-config --modversion libavcodec` = 62.28.102. **Side effect worth knowing:** shell libva is
  now 2.24, so it scans from `__vaDriverInit_1_24` and matches the *system* driver too — VAAPI was
  re-verified with `LIBVA_DRIVERS_PATH` **unset** and still worked. `LIBVA_DRIVERS_PATH` is
  nonetheless **kept**: version alignment is a coincidence that the next NixOS rebuild can undo,
  the env var cannot drift. Verified: `cargo build` green, `cargo check -p wado-client` green,
  `cargo test` green (0 tests), `probe_vaapi` OK, `probe_dmabuf` OK — Tier A zero-copy intact,
  byte-identical output to the pre-update run (31280 / 2911 Annex-B bytes over 10 frames).
  `pkgs.libva` is not on `PKG_CONFIG_PATH` (pre-existing; the compositor links `libav*`, not libva
  directly) — harmless, noted so it isn't re-investigated.

- `2026-09-08` — **Dev shell pins `LIBVA_DRIVERS_PATH`; VAAPI init -5 fixed.** Sessions failed
  at `av_hwdevice_ctx_create(VAAPI, /dev/dri/renderD128) = -5`. Root cause: libva probes the
  driver for `__vaDriverInit_<major>_<minor>` scanning *downward* from its own version. The
  system driver at `/run/opengl-driver/lib/dri` is mesa 26.2 and exports `__vaDriverInit_1_24`
  — **above** the range the dev shell's libva 1.23 scans — so `va_openDriver()` returned -1 and
  ffmpeg surfaced EIO. Fix is one line in `nix/flake.nix`: `LIBVA_DRIVERS_PATH = "${pkgs.mesa}/lib/dri"`,
  pointing libva at the shell's own mesa (26.1.1, exports `__vaDriverInit_1_23`). Note the
  process then runs the VA driver from shell mesa 26.1.1 while GL still dispatches through
  libglvnd to system mesa 26.2 — **verified fine**: `probe_vaapi` and `probe_dmabuf` both pass,
  Tier A zero-copy included. Requires a direnv/`nix develop` reload to take effect.
- `2026-09-08` — **`cargo update` run; smithay deliberately held at `85f83ab`.** All crates.io
  deps refreshed within their existing semver ranges (webrtc 0.17.1→0.17.2, tokio 1.52→1.53,
  etc. — every CLAUDE.md pin respected, no manifest edits). The smithay **git** dep is unpinned,
  so the update moved it to `13738f8`, which broke the build with 38 errors: pointer/touch event
  `time` fields changed `u32` → `InputTime` (`input/pointer.rs`, `input/touch.rs`, `AxisFrame::new`).
  Held back via `cargo update -p smithay --precise 85f83ab...`. **Half-done by design:** the rev is
  pinned in `Cargo.lock` only — `crates/compositor/Cargo.toml` still has a bare `git = ...`, so the
  next `cargo update` reproduces the same 38 errors. Open choice: add
  `rev = "85f83ab61711e725aa1f3481df4f4dc5370e925b"` to the manifest, or do the `InputTime`
  migration as its own change.

- `2026-06-10` — **M4: single Remote ID model + relay-restart resilience (replaces
  server_id+password).** The iteration-1 handshake (relay URL + server ID + password) was "the
  same thing with extra steps"; pivoted to the RustDesk feel: one **Remote ID** is both the
  address and the access token. Format: **9-digit numeric**, displayed `528-491-307`
  (separators stripped by `wado_protocol::relay::normalize_remote_id`; `display_remote_id`
  regroups). Resolution on the server (`crates/server/src/remote_id.rs`, new single-job
  module): `WADO_REMOTE_ID` env override → persisted `$XDG_CONFIG_HOME/wado/remote_id`
  (fallback `~/.config`) → generate (`rand`) + persist + log prominently. **Wire change:**
  `Register { remote_id, display_name }` (no password); the client's `Join` message is
  **deleted** — connecting to `WS /join/:remote_id` IS the join; the relay authenticates
  purely by registry lookup and replies `JoinAccepted`/`JoinDenied` immediately. The future
  **confirmation gate** (CLI approve/deny per incoming peer) slots between `PeerConnected`
  and `JoinAccepted` without another wire change — doc-commented at both hook sites.
  **Resilience:** the server's relay client is now an **auto-reconnect loop** (WebRTC
  API/track/frame-pump built once; per-connection WS connect+register+serve wrapped in
  exponential backoff 1 s→30 s cap, reset after successful registration) — a relay restart
  no longer leaves a dead server logging "auth failed/server not found" forever. Client UI:
  relay mode now needs only **Relay URL + Remote ID** (Server ID + Password fields removed);
  `relay.js` sends no join message and normalizes dashes/spaces. Env vars now:
  `WADO_RELAY_URL` (+ optional `WADO_REMOTE_ID`); `WADO_RELAY_ID`/`WADO_RELAY_PASSWORD` are
  gone. **Security note:** the 9-digit ID is ~30 bits — acceptable only until the relay
  rate-limits join attempts per IP and/or the confirmation gate lands (both tracked in
  TODO.md NECESSARY). Verified live: generate→persist→reuse across server restarts; relay
  killed → backoff 1/2/4/8/16 s → relay restarted → auto re-register under the same ID;
  `/health` shows the server back. Browser join via the client is the user's verification.

- `2026-06-10` — **M4 Iteration 1: relay crate + server relay client + client relay mode.**
  New `crates/relay` (`wado-relay`): standalone axum WebSocket broker (clap CLI, tracing logs,
  `--bind`, `--log-level`, `--max-rooms`). Two WS routes: `GET /register` (server registers ID +
  password, keeps connection open as inbox) and `GET /join/:server_id` (client authenticates and
  triggers room creation). After handshake the relay is a **pure dumb proxy** — any message from
  server side is forwarded verbatim to the paired client, and vice versa. `ServerRegistry`
  (DashMap) tracks registered servers; `RoomStore` (DashMap, one room per server_id) tracks active
  signaling pairs. `/health` returns `{ servers, rooms }` for deployment monitoring.

  New `RelayMsg` enum in `wado-protocol/src/relay.rs` (shared by all three crates): covers
  handshake (`Register/Registered/PeerConnected/Join/JoinAccepted/JoinDenied`), session control
  (`SessionStart/SessionStarted/SessionStop/SessionStopped/SessionLaunch/SessionLaunched/SessionError`),
  log forwarding (`Log`), WebRTC signaling (`SdpOffer/SdpAnswer/IceCandidate`), and keepalive
  (`Ping/Pong`).

  New `crates/server/src/relay_client.rs`: when `WADO_RELAY_URL` env var is set, `main.rs` calls
  `relay_client::start(...)` instead of `website::start(...)`. The relay client builds its own
  WebRTC API + track + frame pump, connects to relay WS via `tokio-tungstenite`, registers, and
  handles forwarded `SessionStart/Stop/Launch + SdpOffer` messages. Tracing log lines are streamed
  to connected clients via `Log { line }` messages. STUN (`stun:stun.l.google.com:19302`) added
  to both `relay_client` and `website::handle_offer` ICE configurations — enables cross-NAT ICE
  without TURN.

  Client: new `relay.js` single-job file (W.relayConnect / W._relayNegotiate / W.relayStop /
  W.relayLaunch). `lifecycle.js` updated to check `relayOpts` and branch between direct HTTP and
  relay WS paths. UI: new "Connection" mode selector (Direct / Via relay); relay fields (Relay URL,
  Server ID, Password) appear when relay mode is selected; the `start` action passes relay opts as
  the third argument to `window.__wado.start()`.

  Environment variables for relay mode on the server: `WADO_RELAY_URL` (e.g. `ws://my-vps:4000`),
  `WADO_RELAY_ID` (default: `"wado-server"`), `WADO_RELAY_PASSWORD` (default: empty string).
  Direct mode (existing HTTP behavior) is unchanged when `WADO_RELAY_URL` is not set.

  Build: clean — all default workspace members compile with zero warnings. Relay binary compiles
  independently (`cargo build -p wado-relay`). Not yet tested end-to-end (no VPS deployment) —
  that is the user's verification step. Security (TLS, argon2 passwords, rate limiting) is Iter 2.

- `2026-06-08` — **M3 finalized (P1): EncoderTuning + zero-latency lock + on-demand
  keyframes + modifier negotiation + live SSE downgrade marker. GStreamer skipped.**
  Three deliverables, all builds green:

  **Stage 1 — Encoder tuning config + zero-latency lock + on-demand keyframes:**
  Research finding: `h264_vaapi` (ffmpeg 8.1) has no intra-refresh option (no
  `AV_CODEC_FLAG2_INTRA_REFRESH`, verified via bindings grep + `ffmpeg -h encoder=h264_vaapi`);
  the `x264` 0.5 crate has no intra-refresh API (only a `zero_latency` bool in `Tune`). The
  previous invariant #7 ("intra-refresh instead of periodic IDR keyframes") was **stale** —
  **on-demand keyframes** are the chosen low-latency strategy: huge `gop_size = fps × 3600`
  (no periodic IDRs), IDR emitted only on viewer-connect and RTCP PLI/FIR (already wired).
  New wire types in `wado-protocol`: `EncoderTuning { zero_latency, keyframe_mode }` (atomic
  sub-struct, default zero_latency=true/OnDemand), `KeyframeMode { OnDemand, Periodic }`.
  `EncoderConfig` gained `zero_latency` + `keyframe_mode`; `EncoderReport` gained `keyframe`
  string ("on-demand"/"periodic"). Both encoder constructors (VAAPI + x264) take `keyframe_mode`.
  x264 always runs `zero_latency=true` (it's the emergency SW fallback — low-latency regardless).
  Client: new **"Encoder settings"** collapsible section (zero-latency checkbox, CBR indicator,
  keyframe-mode select disabled while zero-latency is on, keyframe-interval input visible for
  Periodic mode). Badge tooltip shows the active keyframe mode.

  **Stage 2 — dmabuf modifier negotiation + Tier-A self-test:**
  `DmaTarget::new` now takes `modifiers: Vec<Modifier>` (was hardcoded Linear). In
  `build_tier(VaapiDma)`, the compositor queries `renderer.egl_context().dmabuf_render_formats()`
  for non-Linear Abgr8888 modifiers (GPU-preferred tiled layouts). If any are found: build
  DmaTarget with `[tiled..., Linear]` and run `self_test_dma` (clear→capture→submit) before
  committing — building the tier *is* the probe (invariant #6). On self-test failure, retry with
  Linear and self-test again (regression guard). If preferred is empty: use Linear as before (proven
  path, no self-test, no behaviour change). Zero-copy is never lost to a bad modifier.

  **Stage 3 — Live SSE downgrade marker:**
  New `compositor/src/event.rs`: `pub enum SessionEvent { EncoderChanged(EncoderReport) }`.
  `Wado` gains `pub event_tx: Option<EventSender>` (set at `build()` time). `downgrade_pipeline`
  fires `try_send(SessionEvent::EncoderChanged(tier.report(ec.keyframe_mode)))` after each
  successful tier switch. Server: new `website/events.rs` (one job: drain event_rx, serialize,
  broadcast pre-formatted `"event: encoder\ndata: …\n\n"` frames). `serve_sse` gains a
  `tokio::select!` arm for the encoder broadcast. `logs.js` adds
  `es.addEventListener("encoder", ev => emit({type:"encoder", ...JSON.parse(ev.data)}))`.
  The Rust `"encoder"` handler (already in place from Stage 1) reads `mode`/`pipeline`/`keyframe`.
  GStreamer: explicitly skipped by user decision; the seam (`DmaTarget` exporting a backend-neutral
  `Dmabuf`) is preserved for later if ever needed.

- `2026-06-08` — **M3 Step 2: DMA-BUF zero-copy capture + ffmpeg VAAPI import, 3-tier
  fallback ladder, runtime downgrade-once, client encoder picker + fallback markers.**
  Verified end-to-end on the AMD/`renderD128` box. The hot path is now **zero-copy**: the
  compositor renders into a **GBM dma-buffer** (EGL initialised from the GBM device so
  `GlesRenderer` can `Bind<Dmabuf>`), which is wrapped as a **DRM-PRIME** `AVFrame`, mapped to
  a VAAPI surface (`av_hwframe_map` via `hwmap`) and colour-converted RGBA→NV12 on the GPU by a
  **VAAPI VPP filtergraph** (`scale_vaapi`), then `h264_vaapi`-encoded — no CPU read-back/upload.
  Capture is now an abstraction (`capture::{Frame, CaptureTarget}`) with `MemTarget` (CPU
  `ExportMem`, the robust fallback) and `DmaTarget` (GBM swapchain; exports a Smithay `Dmabuf` —
  the **backend-neutral** hand-off a future GStreamer encoder reuses). The `VideoEncoder` trait
  became `submit(Frame) -> Result<…>`. **3-tier ladder** (`encode/select.rs` `Tier` +
  `tiers_for`, built in `headless::build_pipeline`): **A** `vaapi-dmabuf` → **B** `vaapi-cpu`
  (Step-1 path) → **C** `x264-cpu`; tried top-down, **building the tier *is* the probe**
  (invariant #6 — no separate open-and-cache probe for A/B now; `Auto`→A/B/C, `Hardware`→A/B or
  error, `Software`→C). **Runtime downgrade-once** (`headless::downgrade_pipeline`): a mid-session
  encode `Err` rebuilds one tier down (A→B→C) and forces an IDR, instead of erroring every frame —
  the render loop keeps running. (Native FFI **segfaults** still abort; the per-frame `Result`
  contract + the build-time probe are the guards.) Smithay **`backend_gbm`** feature added (gbm +
  libdrm in the dev shell); **all ffmpeg FFI stays isolated under `encode/ffmpeg/`**
  (`dmabuf_import.rs`, `vpp.rs`, extended `hwcontext.rs`/`mod.rs`). Wire: `EncoderReport` gained
  `pipeline` (`vaapi-dmabuf`/`vaapi-cpu`/`x264-cpu`). **Client**: a proper **Encoder picker**
  (Auto/Hardware/Software, replacing the Debug force-software checkbox) + a **stagebar pipeline
  badge** (green zero-copy / amber HW-cpu-copy fallback / red software) + the persistent SW banner.
  A standalone `examples/probe_dmabuf.rs` exercises Tier A in isolation (confirmed: opens + encodes
  zero-copy). Two real bugs found+fixed in testing: VAAPI min frame size (probe 64→320×240, Step 1)
  and **buffersrc with a HW pix_fmt needs `hw_frames_ctx` set before init** (alloc-unconfigured →
  `av_buffersrc_parameters_set` → `avfilter_init_str`). Deferred (resolved in the next M3 entry
  above): live SSE marker; optimal dmabuf modifiers; GStreamer backend (user-skipped). Remaining
  deferred: NVENC/AMF/QSV/VT tiers; H.265/AV1 (HW-decode-gated). **Browser video sign-off
  (artifact-free under motion on zero-copy) is still the user's.**

- `2026-06-07` — **M3 Step 1: hardware (VAAPI/ffmpeg) encode behind a swappable backend +
  open-and-cache probe + software-fallback banner.** M2 signed off (pointer+keyboard verified).
  Chosen with the user: **ffmpeg/VAAPI for this iteration, GStreamer to follow in parallel**
  (both behind the new `VideoEncoder` trait, selectable in `encode/select.rs`; a future
  `encode/gst/` à la `gst-wayland-display` will be A/B-tested for latency). Binding =
  **`ffmpeg-the-third` 5.0.0+ffmpeg-8.1** — links the dev-shell's system ffmpeg 8.1 (verified:
  `h264_vaapi` opens + encodes on the AMD `amdgpu`/`renderD128` box; bindgen needs clang/
  libclang, present). All ffmpeg FFI is isolated under `encode/ffmpeg/` (per the modularity
  rule). **Staged (correctness-first):** Step 1 (done) feeds the VAAPI encoder from the existing
  **CPU `ExportMem`** capture (RGBA→NV12 on CPU → `av_hwframe_transfer_data` upload → `h264_vaapi`,
  CBR / `max_b_frames=0` / short GOP / Annex-B with inline SPS-PPS on IDR); Step 2 (later) swaps
  in **DMA-BUF zero-copy** (`ExportDma`) — the host-memory path stays per invariant #3. **Probe**
  (invariant #6): actually open an `h264_vaapi` encoder at **320×240** (VAAPI imposes a min frame
  size — 64×64 fails on AMD with EINVAL; a real bug found+fixed in testing) and cache the boolean.
  **Selection:** `Auto` → VAAPI if the probe passes, else x264 + `warn!`; `Hardware` → VAAPI or
  error; `Software` → x264 (a **Debug → "Force software encoding"** toggle exercises this).
  **Fallback notification** (invariant #5): the `/session/start` reply is now JSON
  (`wado_protocol::SessionInfo{ encoder: EncoderReport{ mode, codec, backend } }`); the client
  shows a **persistent banner** ("Software encoding — higher CPU use and latency") when
  `mode==software` (rendered natively in Dioxus; lifecycle.js only emits the mode — UI stays
  native, JS stays glue). New wire types: `EncoderPref{backend: EncoderBackend}` (atomic
  `#[serde(default)]` sub-struct on `SessionConfig`), `SessionInfo`/`EncoderReport`/`EncoderMode`.
  Verified end-to-end on hardware: Auto→`hardware/vaapi`, Software→`software/x264`,
  Hardware→`hardware/vaapi`; the live render loop runs with no tick failures; a standalone
  `examples/probe_vaapi.rs` confirms real 1280×720 encode (forced IDR visibly larger). **Note:**
  GPU/DRM access needs the Bash sandbox disabled (sandboxed runs are SIGKILLed). Deferred to
  later M3 steps: DMA-BUF zero-copy, the GStreamer backend, NVENC/AMF/QSV/VideoToolbox probe
  arms, intra-refresh, H.265/AV1 (HW-decode-gated). See `CHALLENGES.md` M3 list.

- `2026-06-07` — **Modularity policy + M2 input (third slice): full cursorless pointer,
  config-panel sections, single-job module split.** Standing policy (user): **atomically split
  features/settings into independently-distinguishable modules, one job per file**, with thin
  dispatchers. Applied now: compositor `input/` split into `common.rs` (shared clock/coords/
  focus), `keyboard.rs`, `touch.rs`, `pointer.rs`, `window_drag.rs`, and `remote.rs` (thin
  dispatcher); dead `physical.rs` deleted; new-window placement extracted to `placement.rs`.
  Client `bridge.js` split into single-job `js/*.js` files (core/logs/stats/webrtc/input_*/
  overlay/settings/lifecycle), concatenated via `include_str!` in `main.rs`.
  **Mouse = full cursorless pointer** (decided with user): the client routes by
  `PointerEvent.pointerType` — `"mouse"` → real `wl_pointer` motion(hover)/buttons(L/M/R)/axis
  and click-drag (so app CSD titlebar move/resize via the *pointer* grabs is now live), while
  `"touch"`/`"pen"` keep the `wl_touch` gesture FSM. **No cursor is ever rendered** (the render
  path draws only space elements; `cursor_image` is a no-op), so driving `wl_pointer` is
  cursorless by construction. **Scroll fix:** the old arm emitted a bogus per-event `v120=±120`;
  now the client normalizes `wheel` deltas (deltaMode→px × speed × natural-direction) and the
  compositor emits **value-only** axis frames. Window move sources: app CSD, touch long-press-
  drag, and a relocated **Move-mode** toggle (now in Compositor settings). New `SessionConfig`
  behaviour groups (atomic sub-structs): `input: InputConfig{repeat_rate,repeat_delay,
  focus_follows_pointer}`, `window: WindowConfig{placement}` (video fields stay flat). UI: the
  standalone Move button is gone; added collapsible **Compositor settings** (scroll speed +
  natural dir, keyboard repeat, placement, move-mode, focus-follows) and **Debug** (show-touches
  overlay, toggle FPS/ping) sections. Deferred: two-finger/kinetic scroll, discrete-v120 wheel,
  pinch, touch DnD, Super+drag.
- `2026-06-07` — **M2 input (second slice): window dragging, scrolling, right-click, CSD
  touch grabs, multi-touch.** Extended `wado_protocol::InputEvent` with `Scroll`, `Button`
  (`PointerButton::Right`), `WindowDrag`, and `CancelTouch`. **Window dragging** has two
  triggers (user chose both): a client **"Move mode"** toggle and a **long-press-then-drag**
  gesture; both feed `WindowDrag`, handled by a plain move state machine in `input/remote.rs`
  (`Wado::window_move`) — *not* a Smithay grab, to avoid fighting touch routing. **Scrolling**
  is the browser mouse **wheel → `wl_pointer` axis** (two-finger-drag→axis deferred until a real
  multi-touch client exists). **Right-click** is **long-press-then-release-in-place → `BTN_RIGHT`**
  via the pointer. **App-initiated CSD move/resize** now works under touch via new
  `grabs/touch_{move,resize}_grab.rs` (`TouchGrab` impls) wired into `xdg_shell.rs`
  `move_request`/`resize_request` through a new `check_grab_touch`. **Multi-touch** passes
  through per-`id` (the client gesture FSM runs only on the *primary* contact; secondary
  contacts are plain `wl_touch`). **Invariant note:** scrolling and right-click use `wl_pointer`
  internally (the *only* Wayland path for axis/secondary-click) but still render **no cursor** —
  this is the single place the "no mouse pointer" rule is touched, and it was approved. Caveat:
  `wl_touch` cancel is global (all contacts), so a long-press takeover cancels every live
  contact — acceptable this slice. The server is unchanged (it forwards `InputEvent` JSON
  variant-agnostically). Deferred: two-finger/kinetic scroll, pinch-zoom, left/middle button
  emulation, touch DnD (still cancelled).
- `2026-06-07` — **License chosen: GNU AGPL-3.0-only.** Resolves the long-standing licensing
  question. Rationale: the user wants it open source, wants forks/derivatives to stay open, and
  wants to block proprietary monetization. No OSI-approved license can forbid commercial use, so the
  closest fit is strong copyleft — and **AGPL** specifically, because its network clause treats
  running a *modified* version as a hosted service as distribution (closing the SaaS loophole), which
  matters for a streamed-over-the-internet compositor. Considered and rejected: PolyForm Noncommercial
  (source-available, truly bans commercial use, but not OSI-open and doesn't force forks open), Apache
  + Commons Clause (lets forks stay closed), FSL/BUSL (delayed-open, source-available meanwhile). Added
  verbatim `LICENSE` (gnu.org), `license = "AGPL-3.0-only"` on all crates, a README License section,
  and a concise root `README.md`. Note: AGPL is now license-compatible with RustDesk (AGPL) reference
  code, but we still study its design rather than copying — see CLAUDE.md Licensing.
- `2026-06-07` — **M2 input (first slice): end-to-end touch + keyboard, no cursor.** Started the
  reverse input path. **Scope decision (user):** wado synthesizes **only `wl_touch` + `wl_keyboard`,
  no mouse/pointer and no on-screen cursor** — cursor sync over a network feels laggy, and touch/keys
  are cursorless, which *removes* the hardest challenges (cursor sync, high-rate pointer-motion
  flooding, unreliable-channel ordering, pointer emulation). This slice = a **single touch contact**
  (down/motion/up/frame) + keyboard; **multi-touch, scrolling, dragging are explicitly deferred** to
  the next input iteration (Wayland supports them natively; wire format already carries a contact
  `id`). **Design:** new `wado_protocol::InputEvent` (`Touch{id,phase,x,y}` with x,y **normalized
  0..1**; `Key{code,pressed}` with `code` = Linux **evdev** keycode) + `INPUT_CHANNEL="wado-input"`.
  Transport is a **separate, reliable+ordered WebRTC data channel** (never behind video/commands —
  invariant #1): the browser opens it in the offer; the server's `on_data_channel` parses JSON
  `InputEvent`s and forwards them on a **second `calloop::channel`** (parallel to commands) that
  `build` returns via a new `CompositorHandles{commands,input}`. The compositor's `input/remote.rs`
  `handle_remote_input` synthesizes on the seat (`seat.add_touch()` added): touch scaled to the output
  geometry with tap-to-focus (reusing the `physical.rs` focus pattern); keyboard via
  `Keycode::new(evdev+8)` (verified: Smithay/libinput uses evdev+8). The input source is **panic-guarded**
  like the command/render sources (a bad event is dropped, not fatal). **Research grounding:** Wolf/
  Sunshine inject via kernel uinput/uhid virtual devices needing `/dev/uinput`; we inject directly at
  the Smithay seat (no uinput, no privileges) and keep Moonlight's normalized-coordinate idea, doing
  the object-fit letterbox math **client-side** (where element geometry is known) to dodge server DPI
  mapping. **Coordinate-normalization, keycode coverage, tap-to-focus, serial/clock, data-channel
  open/parse, latency** are tracked as live debug items in `INPUT_CHALLENGES.md` (we don't assume them
  away). Client glue (input data channel, touch/keyboard capture, `code`→evdev table, release-on-blur)
  is all in `bridge.js`; keyboard captures only while the `<video>` is focused (page stays usable; tap
  focuses it). Verified: full workspace builds; `cargo check -p wado-client` green; idle boot + a
  `command`-less session start OK with the touch seat. **Not verified here:** the browser tap/type
  end-to-end (needs `dx serve` + a WebRTC client) — user-side.
- `2026-06-07` — **Client: live FPS/ping readout + realtime command launcher.** Two web-client
  additions. (1) A live **FPS · ping** chip above the video: `bridge.js` polls
  `RTCPeerConnection.getStats()` once/sec and emits `{type:"stats", fps, ping}` to Rust (FPS from
  the video inbound-rtp `framesPerSecond`, or computed from the `framesDecoded` delta; ping from
  the selected ICE candidate-pair `currentRoundTripTime`, falling back to remote-inbound-rtp
  `roundTripTime`). (2) The single "Application to launch" config field became a **realtime, launch-only**
  model: **`command` was removed from `SessionConfig`** — sessions **always start blank** — and a new
  **`POST /session/launch`** (JSON-encoded command string) + `CompositorCommand::Launch` spawn arbitrary
  commands into the **running** session, repeatedly. `Wado.app_process: Option<Child>` became
  `app_processes: Vec<Child>` (all killed on stop), so a session is no longer limited to one app. The
  compositor ignores a launch when no session is active; the client's "Launch into session" button is
  enabled only while a session runs. Verified: server stack + `cargo check -p wado-client` green; smoke —
  blank session start, two realtime launches spawn distinct PIDs into the same session, empty command →
  400, launch-without-session ignored with a warning.
- `2026-06-07` — **Compositor extracted into a `crates/compositor` library, decoupled from the
  server.** The single `wado` crate is split: all Smithay/render/encode/capture/sink/conf code
  (the `Wado` god-struct, `headless.rs`, `handlers/`, `grabs/`, `input/`, and the three render
  examples) moved to a new **`wado-compositor`** library; `crates/server` (package `wado`) now
  holds only `website/` (control plane) + `main.rs`. The boundary is **typed channels only**:
  `wado_compositor::build(frame_tx)` creates the `EventLoop`/`Display`/`Wado`, claims the Wayland
  socket (exports `WAYLAND_DISPLAY`), inserts the Wayland + `CompositorCommand` sources, and
  returns `(EventLoop, Wado, CommandSender)`. `main` creates the frame channel, calls `build`,
  hands the `CommandSender` + frame `Receiver` to `website::start`, and runs the loop — the server
  references **no** Smithay type (verified by grep). The old `website::control` module (`ControlCommand`
  + `handle_command`) moved into `crates/compositor/src/control.rs` as `CompositorCommand`; the
  `Sender` is re-exported as `wado_compositor::CommandSender` so the server can name it without a
  smithay dep. **Error type split:** `WadoError` → `CompositorError` (compositor: Renderer/Encoder/
  Capture/Config/session/IO) + `WadoError` (server: WebRTC/JSON/IO + `Compositor(#[from]
  CompositorError)`). **Decision — library split, same process (not a child-process supervisor),
  decided this session:** the compositor is a library in the server's process. Crash isolation is
  interim: `build`'s command source and the render timer are wrapped in `catch_unwind(AssertUnwindSafe)`,
  so a compositor **panic** tears down only the session (logged, reset to idle) and the host process +
  the server's WebRTC/HTTP/SSE connections survive — the viewer just sees a frozen frame until the next
  `Start` (which forces an IDR). **Honest limit:** this covers Rust *unwinding* panics only; a segfault
  in native EGL/GLES/Mesa/x264 still aborts the whole process. Surviving that needs running the
  compositor as a **supervised child process** (server spawns/monitors/restarts it, IPC for frames +
  commands) — deliberately **deferred** as its own milestone; the channel boundary is shaped to make
  that move contained. The build must stay `panic = "unwind"` (the default; verified no `panic=abort`
  profile). **Input (remote pointer/keyboard/touch) — design only this session, no code:** it will NOT
  be more `CompositorCommand` variants; per the "input never rides behind video" invariant it gets its
  own, **separate** `calloop::channel` of `InputEvent`s. The server will parse the WebRTC **data
  channel** into `InputEvent`s and the compositor will synthesize Smithay `PointerHandle`/`KeyboardHandle`/
  `TouchHandle` events on its `seat` (which already lives compositor-side). Grounding: Games-on-Whales
  Wolf runs its Smithay compositor (`gst-wayland-display`) as a library inside a supervised process and
  isolates *applications* in containers — the same library-now / process-isolation-later shape. Verified:
  `cargo build` (server+protocol+compositor) green; `cargo build --examples -p wado-compositor` green;
  server smoke — idle boot OK, `OPTIONS`→204, bad-config→409, `GET /`→API-only hint.

> Append dated entries when a decision changes. Newest first. This is how future sessions know the current state without re-deriving it.

- `2026-09-30` — **Phone UX S4–S7 built; the accessibility tree works, with five non-obvious conditions (S5 spike).** (1) **Bar:** three buttons, `⋯ More · ○ Apps · ◁ Back` left to right (user's choice); smart Back = hide keyboard, else Escape when a popup is open or a dialog is in front, else Alt+Left (`WindowAction::Back`, compositor decides). More becomes ⌨ while an app wants typing and the keyboard is down. (2) **Per-session AT-SPI bus** (`session_env/a11y.rs`): the default `org.a11y.Bus` activation from the private bus reached the **host's** bus (`$XDG_RUNTIME_DIR/at-spi/bus_0`) — a leak — and the registry cannot be activated at all (systemd user unit). wado now runs its own `dbus-daemon --config-file accessibility.conf` + `at-spi2-registryd` per isolated session and hands apps `AT_SPI_BUS_ADDRESS` (+ `QT_LINUX_ACCESSIBILITY_ALWAYS_ON=1`). Needs `WADO_ATSPI` (flake sets it; binaries are in libexec). Without the registry GTK publishes an empty tree. (3) **Querying** (`server/src/a11y.rs`, zbus): GTK4's `GetAccessibleAtPoint` returns null below the first container → hit-test by extents ourselves; `GetChildren` is lazy → use `ChildCount` + `GetChildAtIndex`; some extents are garbage → descend through, never offer; app found by `GetConnectionUnixProcessID` = the window's client pid; grid items are role **"grid cell"**. Measured 8–17 ms per tap query on live nautilus (the gdbus CLI took ~450 ms — never measure through it). (4) **Tap flow:** ask on touch-down so the answer is back by lift (wait ≤70 ms after lift, else plain click); one target near → magnetic snap to its centre; several with a small one (<44 CSS px) → tap-to-lens (3×, live, persistent, outside tap dismisses without clicking); no tree → pixel edge-density heuristic; two-finger tap opens the lens on demand. (5) **Menus (S7):** compositor publishes the focused window's open popup (`hit::MenuSpot`), server reads the showing `menu` → showing items; GTK4 items have **no Name** — the words are on the LABELLED_BY (relation 2) target's **Text**. Rows activate via `Action.DoAction(0)`; verified by opening nautilus's Properties through the tree (`examples/targets_probe.rs`, frame captured). No tree → lens over the real popup.

- `2026-09-29` — **Phone UX S2/S3 amendments — dial drives the strip; won't-fit is one mechanism, not two.** (1) The bottom bar was replaced, at the user's request, by a floating **switcher dial** (`js/switcher.js`): vertical at bottom-right by default, orientation/anchor/3-or-5 visible are settings, spring physics with deliberate overshoot. While held or springing it streams `InputEvent::StripView { pos }` on the *reliable* input channel (a late position after the `None` hand-back would strand the view) and the compositor lays the real columns out at that fractional position with a 24 px `GAP`, so windows slide and bounce with the dial; dialogs left the strip's window list so dial slot i is column i. (2) **Long-press-then-drag now picks up the item under the finger** (left held at the hold point) instead of moving the window, per the user; long-press-and-lift stays right-click; window moving is Move-mode only. (3) **S3 corrected:** the plan's "step down that window's scale, then render-time downscale" is a single mechanism — a lower scale for one app makes it *larger* in its own logical units, so it must be drawn smaller regardless. Implemented as a per-column factor f (`strip::fit_scale`: from `min_size`, or from a refusal once the configure is acked; floor 0.5; monotone): configure `column ÷ f`, preferred fractional scale `session × f` (crisp — the buffer has the column's own pixels), drawn via `RescaleRenderElement` (`scaled.rs`; a creation-time `scale` to `render_elements` does **not** resize surface elements — measured), input mapped `÷ f` in `map_point`. Verified on a real frame with `examples/strip_check.rs` (nautilus at scale 3 in a 240-logical column).

- `2026-09-29` — **Phone UX track (M-P) designed — phone shell with a scrolling strip, translated touch, accessibility-driven precision.** Decided in a grilling session after live phone use was "miserable". Pains: targets too small, windows don't fit, window management, right-click/hover/menus, no gesture translation. Decisions: (1) **Phone shell** on phone-class clients (auto by device class, setting overrides); tablets/desktops keep free windows. (2) **niri-style scrolling strip**, one strip (no vertical workspaces yet): portrait columns are full output width, landscape columns toggle 1/2 or full. Dialogs float over their parent column. (3) **Client draws the chrome** from a new compositor-published window list (id, title, app_id, focused) + per-id window actions + on-demand JPEG thumbnails. (4) **Bottom gesture bar**: swipe ↔ scrolls the strip, swipe ↑ opens the overview, shows each column's app icon (from the existing `.desktop` icon resolver), unfocused tinted; the client animates the swipe locally and the compositor switches on release. (5) **Translated pointer by default**: tap = click, double-tap = double-click (second tap snapped to the first's position, no single-tap delay), long-press = right-click, 1-finger drag = kinetic scroll, 2-finger drag = press-and-drag, pinch unchanged. (6) **Small targets**: detect via the AT-SPI accessibility tree run inside the session (window-relative extents converted with compositor-known window positions), fall back to client pixel heuristics, manual lens always available. Unambiguous small tap → **magnetic snap**; ambiguous → **tap-to-lens** (a persistent ~3× circular lens; next tap inside commits, outside dismisses). (7) **Menus** open as a native phone bottom sheet built from AT-SPI menu items and activated via AT-SPI actions; fallback: real popup + auto-lens. (8) **Won't-fit apps**: step that window's fractional scale down alone (per-surface), render-time downscale only if it still refuses at scale 1. Build order: S1 gestures → S2 window list + strip + bar → S3 per-window scale fit → S5 AT-SPI query → S4/S6 lens + auto-detect → S7 menus.

- `2026-09-29` — **Hygiene pass: fmt baseline clean, `website` split, relay `queue_ms`, first logic tests, docs audit.** (1) One formatting-only commit (`1ff767f`) made the workspace `cargo fmt`-clean; the rule is now *keep it clean*, superseding "format only what you touched". (2) `server/website/mod.rs` (31 KB, four jobs) split by concern into `routes.rs` (request parse + dispatch), `offer.rs` (SDP → peer connection), `pump.rs` (frame pump), `sse.rs` (`/events`), `http.rs` (raw response writers), `watchdog.rs` (viewer-gone stop); `mod.rs` keeps `ServerCtx` and the accept loop. Pure move. (3) The HYGIENE item "relay mode has no timing endpoint" was stale — `TimingRequest`/`Timing` already existed — but relay-mode `queue_ms` was always **0**: the compositor passes `Duration::ZERO` and the relay pump measured the wait and dropped it. The relay pump now keeps the same 1/8 EWMA as the direct pump and overrides `queue_ms` in the `Timing` reply. (4) "There are no tests" was stale (107 existed); added coverage for the encoder tier ladder (`encode/select.rs`) and the `RelayMsg` wire names the JS client hard-codes. Coordinate mapping (`map_point`) needs a Smithay `Space` and the gesture state machine is JavaScript, so neither was cheap pure-logic coverage. (5) Docs audit corrections: `CLAUDE.md` claimed a Pixman fallback (none exists) and that every clock crossing is bounded (only frames are; command/input are unbounded `calloop` channels); `TODO.md` listed as open `PeerDisconnected`, Pages CI/CD, runtime relay URL, NAT detection, pinch and two-finger scroll, all of which are implemented.

- `2026-09-11` — **The 1080p "stutter, freeze and pixelation" was a debug build, not the transport.** The daemon under test was `./target/debug/wado`. Unoptimised, SRTP's per-packet AES and the per-packet `write_sample` await produced 100–200 ms pump stalls, a jitter buffer climbing 19→42 ms, oscillating throughput and fps dipping to 48 at 1728×1080. On the *release* build at the identical config: zero stalls, `avg_queue_ms=0 worst_queue_ms=0`, jbuf flat at 12 ms, fps steady 60 — confirmed by the user ("now the 1080p balanced works better"). **The earlier conclusion that webrtc-rs's serial per-packet write loop was an irreducible residual bottleneck is withdrawn** — it was measured on a debug build. A startup `WARN` under `cfg!(debug_assertions)` now says so, because the symptom reads as a network or encoder fault, not a build problem. Two related findings: the old `write_sample stall` log only fired above 100 ms, which **censors** the data and manufactures a fake "cost is flat across frame sizes" pattern — any future per-packet claim needs uncensored timing first; and webrtc-rs 0.17 has **no pacer, leaky bucket or congestion-controlled send** anywhere in the send path, so "the pacer is throttling us" is not an available explanation. Rejected in the same pass: capping tokio's worker threads. Upstream measured that path worsening with worker count, but release with the default (20 here) has zero stalls, so the change would have been speculative tuning against a solved problem. Still open: `Quality::*` maps to a **flat bitrate ignoring resolution** (4000 kbps at 720p and 1080p alike, ~2× starved at 1080p).

- `2026-06-06` — **Client/server split into a Cargo workspace; client reimplemented in Dioxus (web).** wado is now a workspace of three crates: **`crates/server`** (package `wado`, the compositor + control-plane CLI), **`crates/protocol`** (`wado-protocol`, shared `serde` wire types — `SessionConfig`, `Quality`, endpoint path constants, and the `logfmt` `LEVEL|HH:MM:SS|text` format used by the server's logbus and the client's parser), and **`crates/client`** (`wado-client`, a **Dioxus 0.7 web/WASM** app). `default-members = [server, protocol]` so plain `cargo build` never builds the WASM client on the host. **Client target = web (WASM), decided this session:** it runs in a real browser, so WebRTC/`<video>`/SSE work exactly as the old `index.html`; Dioxus *desktop* was rejected because it embeds WebKitGTK on Linux where WebRTC is broken without a custom build. Web is cross-platform via any browser (incl. mobile); native desktop/mobile come later from the same Dioxus code. **Fully separate processes, decided this session:** the server is now **API-only** — the `GET /` HTML route and `assets/index.html` were removed, and an **`OPTIONS` CORS-preflight** handler was added (the client is a different origin and browsers preflight the JSON POSTs). The client's UI/state/config-building/log-rendering are native Dioxus (signals + rsx); the unavoidable browser-only glue (fetch, WebRTC peer connection, `<video>.srcObject`, SSE, pagehide beacon) is ported into `crates/client/src/bridge.js` and driven via Dioxus `eval` (a long-lived bridge eval reports events back to Rust through `dioxus.send`; Rust kicks actions with one-shot evals into `window.__wado.*`). `SessionConfig`'s old inherent `to_encoder_config` became the free fn `conf::to_encoder_config` (orphan rule, since the type now lives in `protocol`). Verified: `cargo build` (server+protocol) green; `cargo check -p wado-client` type-checks on host (Dioxus 0.7.9); server smoke test — `OPTIONS`→204 w/ CORS, `GET /`→API-only hint, bad-config 409, body-cap 413. **Not verified here:** the wasm build/serve — `dx` (dioxus-cli) and the `wasm32-unknown-unknown` target are absent in the build env (NixOS); building/serving the client (`dx serve -p wado-client --port 8081`) and the browser parity test are user-side steps. Security gate (password/approval + LAN exposure) remains the next milestone.
- `2026-06-06` — **Streaming pipeline hardened; central `WadoError`; live log panel.** Fixed the "stream drops after a few seconds" bug (root cause: the web client treated transient ICE `disconnected` as terminal, and the default 5 s ICE disconnected-timeout made blips fatal). Fixes: client now treats only `failed` as terminal and does a full re-offer with backoff (≤3); server sets `SettingEngine::set_ice_timeouts(15/30/2 s)` (mDNS kept at default `QueryOnly` — correct for localhost, so NOT switched to QueryAndGather); browser RTCP **PLI/FIR → `ForceKeyframe`** and a forced IDR on viewer-connect (fast recovery + instant first picture, via the existing `X264Encoder::force_idr_next`); peer connection now held in `ServerCtx` (not just the RTCP task); a **generation guard** stops a stale viewer's teardown from killing a newer session; request body capped (256 KB) and `/session/start` reply bounded by a 3 s timeout; `pagehide`→`sendBeacon('/session/stop')`. Introduced a **central `WadoError`** (`src/error.rs`, thiserror, `Send+Sync`) across `website`/`headless`/`encode`/`capture` with `crate::Result`. Added a **live log panel**: a `tracing` `Layer` (`src/website/logbus.rs`) broadcasts all tracing output over an SSE `GET /events` endpoint to a collapsible, color-coded panel in the web client (note: libx264's C-stderr lines bypass tracing and don't appear). Reference checks: webrtc-rs `examples/broadcast` (RTCP/PLI), `SettingEngine` docs, pion `sfu-ws`. Verified: build green; SSE backfill+stream, body-cap 413, bad-config 409, lifecycle all pass headlessly. Browser "no longer drops after seconds" pending user confirmation.
- `2026-06-06` — **Control-plane / on-demand session model added (new `website` module).** wado no longer starts a compositor session at boot. `main` now brings up only an always-on HTTP **control server** (new `src/website/`) that serves a native web client and waits. The operator picks options in the browser and clicks **Start**, which spins up the compositor session (EGL, encoder, client-sized `Output`, render timer, launches the chosen app); **Stop** — or the viewer's WebRTC connection closing — tears it all down, so an idle wado consumes ~no GPU/CPU. Options surfaced (RustDesk-derived): resolution, fps, quality (*reactivity/balanced/quality/custom-bitrate* → bitrate+preset+keyframe), free-form launch command, and advanced (x264 preset, keyframe interval). The WebRTC `API`/track/pump/`/offer` moved out of `src/sink/webrtc.rs` into `website`; the per-session sink is now a thin `ChannelSink` feeding the pump. tokio↔calloop bridge: a `calloop::channel` carries `Start`/`Stop` commands (reply via tokio `oneshot`); frames ride the existing bounded drop-on-full mpsc. **Security gate (password/approval) is the NEXT step** — until then the control server binds **`127.0.0.1`** because the launch command is free-form (an RCE surface). Lifecycle verified end-to-end via the control API (idle→start→409-on-double-start→stop→restart at a different resolution; app process killed on stop). Sits alongside M2; not itself a milestone item.
- `2026-06-05` — **Transport pivoted to WebRTC (webrtc-rs), pulled forward from M4.** The raw-UDP sink (`src/sink/udp.rs`) corrupted the stream under heavy motion: it sent one whole encoded frame per UDP datagram with no framing, so frames over the ~64 KB IPv4 UDP limit were silently dropped and any frame over the path MTU was IP-fragmented (a single lost fragment kills the frame). The decoder then rendered P-frames referencing data it never received → the "pixels mixing" artifact. Confirmed transport-only: `FileSink` output (`examples/capture_to_disk.rs`) plays back cleanly. **Decision:** adopt **webrtc-rs v0.17.1** (stable; NOT 0.20.0-alpha.1, the Sans-IO line) for video transport now. WebRTC gives MTU-sized RTP packetization (FU-A), DTLS-SRTP, ICE, and congestion control for free — fixing the bug and letting us defer hand-rolled RTP/FEC/NAT. New `src/sink/webrtc.rs` (impl `FrameSink`) runs a tokio runtime on its own thread, bridged to the sync calloop render loop by a bounded drop-on-full channel. Signaling = built-in HTTP (`POST /offer` → SDP answer, `GET /` → `assets/webrtc_test.html`); verify in a browser at `http://localhost:8080`. The UDP sink was removed; `FileSink` stays. **Resolves the open question** "Custom UDP/RTP vs. webrtc-rs" in favor of webrtc-rs. Build passes; browser verification pending user confirmation.
- `2026-06-05` — **M0 threshold met and verified.** `weston-terminal` rendered inside the nested Wado compositor; keyboard and mouse input confirmed working. Advancing to M1.
- `2026-06-05` — M0 implementation complete. Smithay compositor ported from smallvil; renamed to `Wado`. Used a git dependency on Smithay master (not crates.io 0.7.0) to avoid path/API drift. Module structure: `state`, `winit`, `handlers/{mod,compositor,xdg_shell}`, `grabs/{move_grab,resize_grab}`, `input/{physical,virtual_input}`. `input/virtual_input.rs` is a stub hook-point for M2 phone input injection.
- `2026-06-05` — Initial plan established from the architecture research. GStreamer chosen as the first video-pipeline implementation (with a documented exit to direct encoder bindings). Custom UDP/RTP+FEC (Moonlight model) is the transport target, WebRTC the fallback. H.264 is the default codec. "Be the compositor" model chosen over desktop-mirroring.

## Open Questions (resolve and move to the Decision Log)
- GStreamer pipeline vs. direct ffmpeg/encoder bindings — confirm after M3 on real target hardware.
- Whether to ever support model B (desktop mirroring) as a separate mode. *Resolved for macOS
  only (`2026-10-17`, Milestone MAC); still open on Linux.*


---

# Relay target-state architecture (migrated from TODOS.md, `2026-09-11`)

> Moved here when `TODOS.md` was replaced by `TODO.md`. `TODO.md` is state — what is done
> and what is next; this is architecture, so it belongs with the rest of the plan. The
> iteration table below is the M4 roadmap; iteration 1 is complete (see the `2026-06-10`
> Decision Log entries), 2–4 are open and tracked in `TODO.md`.

### Architecture (target state)

```
┌──────────────────────────────────────────────────────────────┐
│  GitHub Pages                                                │
│  wado-client (Dioxus WASM)                                   │
│  - Direct mode:  POST /offer → wado-server (LAN only)        │
│  - Relay mode:   WS → wado-relay → broker SDP/ICE            │
└──────────────┬───────────────────────────────────────────────┘
               │ WebSocket (wss://)
               ▼
┌──────────────────────────────────────────────────────────────┐
│  wado-relay  (VPS, public IP)                                │
│  - Server registry:  wado-server instances register here     │
│  - Signaling broker: proxies SDP offer/answer + ICE cands    │
│  - Auth: Remote ID lookup (join path = the token); future    │
│    confirmation gate (server approves each peer)             │
│  - TURN relay:       media relay for symmetric NAT fallback  │
└──────────────┬───────────────────────────────────────────────┘
               │ WebSocket (ws/wss) — register + signal
               ▼
┌──────────────────────────────────────────────────────────────┐
│  wado-server  (behind NAT / laptop)                          │
│  - Registers with relay on startup under its Remote ID       │
│    (persisted 9-digit token; auto-reconnect w/ backoff)      │
│  - Receives SDP offers forwarded by relay                    │
│  - Answers via relay; ICE/DTLS-SRTP takes over for media     │
│  - Direct mode still works on LAN (no relay needed)          │
└──────────────────────────────────────────────────────────────┘
```

### Iterations

| # | Scope | Deliverable |
|---|---|---|
| **1** | relay scaffold + signaling | wado-relay binary; server registers; client joins; SDP/ICE forwarded; end-to-end test over relay |
| **2** | security gate | ~~password hashing~~ superseded `2026-10-02`: daemon-side approval on a connected device (`peer_accept`/`peer_reject`), relay join rate limit, TLS via Caddy |
| **3** | TURN relay | ICE TURN fallback built into wado-relay; symmetric-NAT connect verified |
| **4** | GitHub Pages CI/CD | GH Actions builds + deploys Dioxus WASM; client picks relay URL at runtime |

---

## Implementation Notes (concerns surfaced mid-planning)

- The relay's WebSocket messages carry SDP blobs which can be large (~2 KB). Ensure the WS frame size limit is set generously (e.g., `max_message_size = 64 KB`).
- STUN with Google's servers works globally but adds an external dependency. For a fully self-hosted setup, deploy a coturn instance as both STUN and TURN.
- The Dioxus WASM build (`dx build`) produces a `dist/` folder. GitHub Pages needs the base URL configured in `Dioxus.toml` (`base_path = "/<repo-name>/"`) or a custom domain.
- `wado-relay` should be buildable with plain `cargo build -p wado-relay` with no GPU/DRM deps — it is a pure-async network binary.
- The relay keeps no persistent state. Since `2026-10-02` a seat outlives its sockets for the grace period, but it lives only in memory and is rebuilt from reconnects (stable `instance_key`/`client_key`), so a relay restart loses nothing.
- Password in `JoinMsg` is sent over the WS in plaintext JSON → **TLS is mandatory before production**. On localhost/LAN without TLS, this is acceptable for development only.
- `webrtc-rs` `RTCConfiguration` takes `ice_servers: Vec<RTCIceServer>`. Adding STUN is a one-line change; adding TURN requires credentials.
