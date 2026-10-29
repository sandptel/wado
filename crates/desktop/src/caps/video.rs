// SPDX-License-Identifier: AGPL-3.0-only
//! Native desktop streaming — **absent in this build**, and said so as a capability rather than
//! a button that does nothing.
//!
//! The plan (not implemented):
//!
//! 1. **Signalling and E2E.** Dial the computer's relay as a *device* does: the same
//!    `wado_protocol::relay_wire` handshake and `e2e` envelope the web client speaks, with this
//!    app holding its own device key in `~/.config/wado-desktop/`. The app must first be
//!    *paired* like any device (QR link or typed code + check words), so a `pair` flow lands
//!    here before video does.
//! 2. **Media.** webrtc-rs on the workspace's pinned 0.17 line (CLAUDE.md "Pins"), receive-only
//!    H.264 video track + Opus audio + the input data channels. Input goes on its own channel and
//!    never waits behind video (invariant 1).
//! 3. **Decode.** ffmpeg (8.x ABI, already linked by the compositor) with VA-API hardware decode;
//!    software decode as the fallback, named in the UI (invariant 5's spirit on the receive side).
//! 4. **Present.** Decoded frames into a `slint::Image` — first by copying NV12→RGBA into a
//!    `SharedPixelBuffer` (simple, always works), then zero-copy by importing the VA surface as a
//!    DMA-BUF into the femtovg GL context (`slint::BorrowedOpenGLTextureBuilder`). Stage Black
//!    behind it, whatever the scheme.
//! 5. **Threads.** Decode on its own thread; the UI is told "a frame is ready" through
//!    `upgrade_in_event_loop`, never blocking the event loop — the platform layer's
//!    `VideoBackend { thread: Any|Main }` contract, consumed rather than implemented here.
//! 6. **Sizing.** The session is spawned at this window's size (a fresh `Output`, invariant 8),
//!    and re-asked on resize.

use super::Cap;

/// A native video receiver, once there is one.
pub struct Receiver;

pub fn native() -> Cap<Receiver> {
    Cap::off(
        "This build opens a computer's desktop in your browser, through the web client. Streaming \
         it into this window — WebRTC with hardware decode, drawn straight into the app — is \
         planned next; it needs this app paired as a device first.",
        &["webrtc-rs 0.17 + ffmpeg vaapi (planned)"],
    )
}
