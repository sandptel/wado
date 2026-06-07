# wado

A headless **Wayland compositor** (Rust / Smithay) that streams its display over **WebRTC**
at low latency and accepts remote **mouse, keyboard, and touch** input back — drive a real
Linux graphical session from a browser (and, later, a phone). wado is its *own* compositor:
apps run **inside** it; it is not a mirror of an existing desktop.

Video goes out **hardware-accelerated and zero-copy** where possible: the compositor renders
straight into a GPU DMA-BUF, imports it into VA-API, colour-converts on the GPU, and encodes
with `h264_vaapi` — no CPU round-trip — falling back gracefully to CPU paths when the hardware
can't. On a capable GPU it sustains high frame rates at very low CPU.

> Status: early / pre-alpha, under active development. **localhost / LAN only — no auth yet.**

## Highlights
- **Zero-copy hardware H.264.** Render → DMA-BUF → VA-API (DRM-PRIME import + on-GPU RGBA→NV12
  VPP) → `h264_vaapi`. Encoders are tuned for latency (CBR, no B-frames, short GOP).
- **A 3-tier fallback ladder, never a hard failure.** `vaapi-dmabuf` (zero-copy) →
  `vaapi-cpu` (CPU read-back + VA-API upload) → `x264-cpu` (software). Tiers are *probed by
  actually opening them*; a runtime encode failure **downgrades one tier and keeps streaming**.
  The browser shows which path is live (a green/amber/red pipeline badge) and warns on software.
- **First-class input, no on-screen cursor.** A real mouse drives a cursorless `wl_pointer`
  (hover, L/M/R buttons, wheel scroll, click-drag, CSD titlebar move/resize); a touchscreen
  drives `wl_touch` gestures (tap, long-press right-click, long-press-drag move) — routed
  client-side by pointer type. Keyboard too. Input rides a **separate** low-latency WebRTC data
  channel (never queued behind video).
- **Per-client sizing.** Sessions spawn an output at the resolution/fps you ask for; touch
  coordinates map 1:1.

## Layout (Cargo workspace)
- `crates/compositor` — the Smithay compositor as a library: GLES render, DMA-BUF/host-memory
  capture, and software (x264) + hardware (VA-API via ffmpeg) H.264 encode behind one trait.
- `crates/server` — control plane (HTTP / WebRTC / SSE) that drives the compositor over typed
  channels; binary `wado`. Holds no Smithay type.
- `crates/protocol` — shared wire types.
- `crates/client` — Dioxus **web (WASM)** client.

## Build & run
The compositor links **system ffmpeg (≥ 8)** and uses GBM/DRM for the zero-copy path, so a host
build needs: **ffmpeg dev libs, gbm, libdrm, pkg-config, and clang/libclang** (for bindgen).

Server (plain host cargo):
```
cargo run -p wado            # idle control server on http://127.0.0.1:8080
```
Client (needs the `dx` CLI — `dioxus-cli` — and the `wasm32-unknown-unknown` target):
```
dx serve -p wado-client --port 8081   # then open http://localhost:8081
```
In the client: pick **resolution / fps / quality / encoder** (Auto · Hardware · Software) →
**Start**; type a command + **Launch** to spawn apps into the session (any number). A real
**mouse** drives the cursorless pointer and a **touchscreen** drives gestures; click the video
then type for keyboard. The hardware (VA-API) path needs a GPU with a DRM render node; with none
(or if you pick Software) it falls back to x264 and shows a banner.

Build gate is `cargo build` (the WASM client is built with `dx`, not host cargo).

## Status & roadmap
- **Done:** headless render → **hardware (VA-API) / software (x264) H.264** → WebRTC video in the
  browser; **DMA-BUF zero-copy** capture + a 3-tier fallback ladder with runtime downgrade;
  on-demand sessions; realtime command launch; **full remote input** (cursorless pointer + touch
  gestures + keyboard) on a separate channel; live FPS / ping readout.
- **Next:** a **security gate** (auth / approval) before any non-localhost use; a parallel
  **GStreamer** pipeline for latency A/B; **internet transport** (rendezvous + NAT traversal);
  dynamic per-client resolution; a native mobile client.

See `WADO_PLAN.md` for the full architecture and milestone plan.

## License
**GNU AGPL-3.0-only** (see [`LICENSE`](LICENSE)). Copyright (C) 2026 Sandeep Patel
([@sandptel](https://github.com/sandptel)). wado is free/open-source software; if you modify
it — including running a modified version as a network service — you must release your full
source under the same license. Forks and hosted deployments stay open.
