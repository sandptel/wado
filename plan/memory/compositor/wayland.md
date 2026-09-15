# compositor — Wayland

Smithay, headless, GLES2/Glow renderer, Pixman as the no-GPU fallback. Not the winit backend
(slated for deprecation). Smithay tracks a **git revision pinned in the lockfile only** —
upstream `main` breaks API regularly.

## Two clocks

The compositor is a **synchronous calloop loop**; the transport is **async tokio**. Every
crossing is a bounded, drop-on-full channel (`FRAME_CHANNEL_CAPACITY = 2`, deliberately
shallow). The render tick never blocks on the network.

`flush_clients()` runs in the loop's **post-dispatch** callback, not at the tail of the
render tick. Flushing only on render quantises every remote input to the frame period with a
variable phase — which is exactly what "remote input feels jittery" is.

## Scaling

`wp-fractional-scale-v1` + `wp-viewporter` are both advertised **unconditionally at startup**
— a client decides how to draw when it binds, long before a session sets a scale, and a
global that appears later is one most toolkits never look for again.

Without fractional scale, `wl_output.scale` is the only channel and it is an integer: a
client asked for 1.5 is told 2, draws at 2×, is composited at 1.5×, and its buffer overhangs
its own area — visible as clipped UI elements. That was the reported "cutouts over
application elements" at high scaling.

⚠️ **But the protocol is advertised and then not used.** `headless.rs` rounds the scale before
both the output state and the surface push, so a client asking 1.25 is handed 1.0 — the
fractional path carries an integer. Details and the `Scale::Custom` fix are in
[`wayland-protocols.md`](wayland-protocols.md); do not re-derive them here.

## Outputs

**Per-client resolution comes from a fresh `Output`.** Wayland cannot un-advertise a mode, so
sizing to a client means a new output, never a mutated one. Touch then maps 1:1. The output's
global is removed on session stop or stale outputs stay advertised.

## Input

- Touch is the primary remote input; pointer capability exists but is never driven (no
  on-screen cursor).
- Scroll uses `AxisSource::Finger` with a terminating axis-stop, so kinetic scrolling ends
  when the finger lifts. `AxisSource::Wheel` gives discrete-notch semantics instead.
- Motion rides an **ordered** zero-retransmit channel: absolute positions cannot tolerate
  reordering.
- Compositor-driven window moves are a plain state machine fed by `WindowDrag`, never a
  Smithay touch grab — a grab fights touch routing.

## Crash isolation

The compositor is a **library driven only through typed channels**; the server crate holds no
Smithay type and the compositor holds no network type. That boundary is what makes a future
child-process supervisor possible — do not reach across it.

Panics in the render and command paths are caught per session. **A panic is contained; a
segfault is not** — native crashes in the graphics or encoder stack still take the process
down.
