# wado

A headless **Wayland compositor** (Rust / Smithay) that streams its display over **WebRTC**
at low latency and accepts remote **input** back — drive a real Linux graphical session from
a browser (and, later, a phone). wado is its *own* compositor: apps run **inside** it; it is
not a mirror of an existing desktop.

> Status: early / pre-alpha, under active development. localhost/LAN only — no auth yet.

## Layout (Cargo workspace)
- `crates/compositor` — the Smithay compositor (render + x264 encode) as a library
- `crates/server` — control plane (HTTP / WebRTC / SSE) that drives the compositor; binary `wado`
- `crates/protocol` — shared wire types
- `crates/client` — Dioxus **web (WASM)** client

## Build & run
Server (plain host cargo):
```
cargo run -p wado            # idle control server on http://127.0.0.1:8080
```
Client (needs the `dx` CLI — `dioxus-cli` — and the `wasm32-unknown-unknown` target):
```
dx serve -p wado-client --port 8081   # then open http://localhost:8081
```
In the client: pick resolution / fps / quality → **Start**; type a command + **Launch** to
spawn apps into the session (any number). Interact with **mouse-as-touch** (clicks/drags
become touch events — no cursor) and **keyboard** (click the video, then type).

Build gate is `cargo build` (the WASM client is built with `dx`, not host cargo).

## Status & roadmap
- **Done:** headless render → H.264 (x264) → WebRTC video in the browser; on-demand sessions;
  realtime command launch; live FPS / ping readout.
- **Now:** remote input — touch (from the mouse) + keyboard.
- **Next:** input polish (multi-touch, scrolling, dragging); a **security gate** (auth /
  approval) before any non-localhost use; hardware encode; internet transport (rendezvous + NAT).

See `WADO_PLAN.md` for the full architecture and milestone plan.

## License
**GNU AGPL-3.0-only** (see [`LICENSE`](LICENSE)). Copyright (C) 2026 Sandeep Patel
([@sandptel](https://github.com/sandptel)). wado is free/open-source software; if you modify
it — including running a modified version as a network service — you must release your full
source under the same license. Forks and hosted deployments stay open.
