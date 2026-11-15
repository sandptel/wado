# wado — live challenges checklist

## M3 — hardware encode (active)

Step 1 added VAAPI (ffmpeg) hardware encode. **Step 2 (done)** added DMA-BUF **zero-copy**
capture + import (render→GBM dmabuf→DRM-PRIME→VAAPI VPP RGBA→NV12→`h264_vaapi`), a **3-tier
ladder** (`vaapi-dmabuf` → `vaapi-cpu` → `x264-cpu`, building-the-tier-is-the-probe), **runtime
downgrade-once**, and a client encoder picker + stagebar pipeline badge. Watch:

- [ ] **Zero-copy colour correctness (Tier A).** Symptom: tint/swapped channels on the dmabuf
      path vs the CPU path. Cause: the DRM layer fourcc (`Abgr8888`) ↔ the DRM-frames `sw_format`
      (`AV_PIX_FMT_RGBA`) mapping, or the VPP `scale_vaapi=format=nv12` range/matrix. Compare a
      Tier-A vs Tier-B frame side by side. Look: `encode/ffmpeg/{dmabuf_import,vpp}.rs`.
- [ ] **DMA-BUF modifier minefield.** We allocate **Linear** dmabufs (`capture/dmabuf.rs`) for
      portability; if a driver rejects Linear render targets or the VAAPI import, Tier A fails its
      build → the ladder drops to `vaapi-cpu`. To chase performance later, try optimal/explicit
      modifiers and confirm both the GLES render *and* the VAAPI import accept them.
- [ ] **buffersrc HW-format init order.** A HW input pix_fmt (DRM_PRIME) needs `hw_frames_ctx`
      set *before* the filter inits — alloc unconfigured (`avfilter_graph_alloc_filter`) →
      `av_buffersrc_parameters_set` (format + hw_frames_ctx) → `avfilter_init_str`. Getting this
      wrong returns `-22`. Look: `encode/ffmpeg/vpp.rs`.
- [ ] **Runtime downgrade-once.** A mid-session encode `Err` rebuilds one tier down and forces an
      IDR (`headless::downgrade_pipeline`); the render loop continues. Verify a forced Tier-A fault
      degrades to `vaapi-cpu` then `x264-cpu` without crashing. **Caveat:** the client badge only
      updates at Start (a live SSE marker for runtime downgrade is deferred — a prominent WARN log
      covers it). Native FFI **segfaults** are NOT caught — the per-frame `Result` + build-time
      probe are the guards.
- [ ] **Swapchain depth / latency.** `DmaTarget` uses a small GBM swapchain (smithay max 4) with
      full-redraw each tick and per-slot cached `Dmabuf` (renderer EGLImage cache hit). If the GPU
      stalls or buffers pile up, revisit depth.

- [ ] **VAAPI minimum frame size.** Symptom: `Auto` silently picks software; log shows
      `avcodec_open2 ... -22` / "Hardware does not support encoding at size NxN". Cause: probing
      (or a session) below the driver's min (AMD: 128–4096 per side). The probe uses 320×240;
      keep session resolutions ≥128. Look: `encode/ffmpeg/probe.rs`.
- [ ] **Colour correctness (HW vs SW).** Symptom: tints/washed-out colour vs the x264 path.
      Cause: `convert::rgba_to_nv12` uses BT.601 **limited** range (matching `rgba_to_i420`); the
      encoder/decoder must agree on range/matrix. Verify HW and SW frames look identical. Look:
      `encode/ffmpeg/convert.rs`.
- [ ] **Zero-latency RC tuning.** First-pass: CBR (`rc_mode=CBR`, `rc_max=rc_min=bitrate`,
      `rc_buffer≈1s`), `max_b_frames=0`, `async_depth=1`, `gop_size=keyframe_interval`. No
      intra-refresh yet (deferred). Watch for bitrate overshoot / latency spikes at IDR; tune
      `rc_buffer_size` down if spiky. Look: `encode/ffmpeg/mod.rs` `open_h264_vaapi`.
- [ ] **Forced IDR.** A new viewer / RTCP PLI sets `pict_type = I` on the next frame. Verify the
      browser recovers fast after loss and a late joiner syncs (SPS/PPS are inline — no global
      header). Look: `encode/ffmpeg/mod.rs` `send_and_drain`.
- [x] **CPU convert+upload cost** — removed from the hot path on Tier A (zero-copy). Still present
      on the `vaapi-cpu` fallback tier; measure if that tier is in use.

- [ ] **VAAPI minimum frame size.** Symptom: `Auto` silently picks software; log shows
      `avcodec_open2 ... -22` / "Hardware does not support encoding at size NxN". Cause: probing
      (or a session) below the driver's min (AMD: 128–4096 per side). The probe uses 320×240;
      keep session resolutions ≥128. Look: `encode/ffmpeg/probe.rs`.
- [ ] **Colour correctness (HW vs SW).** Symptom: tints/washed-out colour vs the x264 path.
      Cause: `convert::rgba_to_nv12` uses BT.601 **limited** range (matching `rgba_to_i420`); the
      encoder/decoder must agree on range/matrix. Verify HW and SW frames look identical. Look:
      `encode/ffmpeg/convert.rs`.
- [ ] **Zero-latency RC tuning.** First-pass: CBR (`rc_mode=CBR`, `rc_max=rc_min=bitrate`,
      `rc_buffer≈1s`), `max_b_frames=0`, `async_depth=1`, `gop_size=keyframe_interval`. No
      intra-refresh yet (deferred). Watch for bitrate overshoot / latency spikes at IDR; tune
      `rc_buffer_size` down if spiky. Look: `encode/ffmpeg/mod.rs` `new`.
- [ ] **Forced IDR.** A new viewer / RTCP PLI sets `pict_type = I` on the next frame. Verify the
      browser recovers fast after loss and a late joiner syncs (SPS/PPS are inline — no global
      header). Look: `encode/ffmpeg/mod.rs` `force_idr_next`/`encode_frame`.
- [ ] **CPU convert+upload cost.** Step 1 does RGBA→NV12 on the CPU then `av_hwframe_transfer_data`
      every frame — the thing Step 2 (DMA-BUF) removes. If CPU is the bottleneck before then,
      that's expected. Measure encode time per tick.
- [ ] **Unsafe FFI soundness.** All ffmpeg FFI is in `encode/ffmpeg/`. A native segfault there is
      NOT caught by the render-tick panic guard (only Rust unwinds are). Frames are freed on every
      path incl. errors; the encoder ctx is freed in `Drop`. Re-audit when touching that code.
- [ ] **Sandbox + GPU.** Running anything that opens `/dev/dri/renderD*` (the probe, a session,
      `examples/probe_vaapi.rs`) needs the Bash sandbox disabled — sandboxed runs are SIGKILLed
      (exit 144), not a code bug. Also: `pkill -f 'target/debug/wado'` matches the *test script's
      own* command line — use `pkill -x wado` or the captured PID.

Deferred (later M3): the parallel GStreamer backend (`encode/gst/`, reusing `DmaTarget`'s
`Dmabuf`), live SSE marker for runtime downgrade, optimal dmabuf modifiers, NVENC/AMF/QSV/VT
tiers, intra-refresh, H.265/AV1 (HW-decode-gated).

---

# Input — live challenges checklist (M2 — signed off; kept for reference)

A **temporary, living** checklist for the M2 input slice (touch + keyboard, no cursor). We do **not**
assume these problems don't exist; we actively watch for the listed symptom and fix it where noted.
Tick items as they're confirmed handled. Delete this file (or fold into `WADO_PLAN.md` §3.4 Gotchas)
once the input path is stable.

Scope reminder: third slice makes a **mouse drive a full cursorless `wl_pointer`** (hover, L/M/R
buttons, wheel scroll, click-drag, CSD titlebar move/resize) while a **touchscreen keeps `wl_touch`
gestures** (tap / long-press right-click / long-press-drag move), routed client-side by
`pointerType`. Scroll is fixed to **value-only** axis frames with client delta-normalization.
Adds configurable keyboard-repeat / window-placement / focus-follows-pointer / scroll-speed and a
Debug overlay. Still **no on-screen cursor**. Two-finger/kinetic scroll, discrete-v120 wheel,
pinch, and touch DnD remain deferred.

## Active — debug as they appear

- [ ] **Keyboard `code`→evdev coverage.** Symptom: a key does nothing or types the wrong character.
      Cause: missing entry in the `KeyboardEvent.code` → Linux evdev table, or wrong code. Look:
      `crates/client/src/bridge.js` keycode map. Unknown codes must log (not silently drop).
- [ ] **Modifiers, key-repeat, stuck keys.** Symptom: Shift/Ctrl combos wrong; key "sticks" if a
      keyup is missed (e.g. lost focus mid-press). Cause: browser doesn't deliver keyup when unfocused.
      Mitigation: release all pressed keys on blur/visibilitychange. Look: `bridge.js` keyboard handlers;
      compositor relies on matched down/up. Repeat is driven by the compositor's xkb (delay 200/rate 25).
- [ ] **Touch coordinate calibration (letterbox math).** Symptom: taps land offset/scaled wrong,
      worse on non-16:9. Cause: normalizing against the element box instead of the rendered video
      *content* rect (`object-fit: contain`). Look: `bridge.js` normalize fn (uses videoWidth/Height vs
      element rect). Verify on a letterboxed and an exact-fit resolution.
- [ ] **Tap-to-focus correctness.** Symptom: typing goes to the wrong window / nowhere after a tap.
      Cause: keyboard focus not set to the surface under the touch-down, or window not raised. Look:
      `crates/compositor/src/input/remote.rs` Touch::Down arm (reuses the physical.rs focus pattern).
- [ ] **Smithay serial/time correctness.** Symptom: double-tap not detected, drag thresholds off,
      events ignored. Cause: bad/sated serials or non-monotonic `time`. Look: `remote.rs`
      (`SERIAL_COUNTER.next_serial()`, `start_time.elapsed().as_millis()`).
- [ ] **Mouse must map to touch.** Symptom: nothing happens on a desktop mouse click (no `wl_touch`).
      Cause: `touchstart/move/end` don't fire for a mouse. Fix in place: client captures **Pointer
      Events** (`pointerdown/move/up`) and translates each to a touch contact. Look: `bridge.js`
      `setupInputCapture` (pointer listeners + `setPointerCapture`). Pen/touch ride the same path.
- [ ] **webrtc-rs data-channel open + parse.** Symptom: input never arrives. Confirm via the log
      panel: `info` log "input data channel \"wado-input\" open" should appear on connect; with
      `RUST_LOG=debug`, each event logs server-side ("input event") and compositor-side
      ("synthesizing input"). Cause if absent: server didn't get `on_data_channel` (label mismatch),
      channel not in the offer, or JSON parse error. Look: `crates/server/src/website/mod.rs`
      `on_data_channel`; `bridge.js` `createDataChannel("wado-input")` before `createOffer`.
- [ ] **SCTP `failed to handle_inbound: ErrChunk` warning.** Seen in logs; usually benign (a chunk
      during association setup/teardown) and the channel still opens. Treat as a problem **only if**
      the "input data channel open" log never appears or events don't arrive.
- [ ] **Input glass-to-glass latency.** Symptom: taps/keys feel laggy. Don't assume — measure
      (capture→datachannel→synthesize→render→encode→decode→display). The stage's FPS·ping shows
      transport RTT; add trace timing if input feels slow.
- [ ] **Page usability vs capture.** Symptom: can't scroll/use the client page; or keys leak to the
      page. Cause: capture gating. Look: `bridge.js` gates keyboard capture on stage focus; touch uses
      `preventDefault` only over the video. Note: `wheel` over the video now `preventDefault`s, so the
      page won't scroll while the cursor is on the video — expected.
- [ ] **Gesture FSM disambiguation.** Symptom: a quick drag is mistaken for a tap, a long-press
      fires when you meant to scroll, or the window jumps on move-start. Cause: thresholds
      (`HOLD_MS=500`, `MOVE_THRESHOLD=8`) and the tap/touch/held/move transitions. Look: `bridge.js`
      `setupInputCapture` pointer handlers. The FSM runs on the **primary** contact only.
- [ ] **Window move feel.** Symptom: window doesn't follow the contact, jumps, or sticks. Cause:
      `Wado::window_move` delta math / lost `WindowDrag{Up}`. Look: `remote.rs` `WindowDrag` arm.
      `window_move` is cleared on session stop; a missed `Up` would leave a window "stuck" to drags.
- [ ] **Scroll delivery + speed.** Symptom: nothing scrolls, or scrolls too fast/slow/wrong axis.
      Cause: pointer not focused at the wheel point, or pixel-delta→axis scaling. Look:
      `input/pointer.rs` `pointer_scroll` (value-only axis, no v120); `js/input_pointer.js` `wheel`
      (deltaMode→px × speed × natural-dir). Browsers vary `deltaMode`. Some apps want discrete v120 —
      deferred; tune with the scroll-speed slider for now.
- [ ] **Cursorless pointer hover flooding.** Symptom: lag/jank from a torrent of `pointer_motion`
      over the reliable channel on mouse move. Mitigation in place: `js/input_pointer.js` coalesces
      hover to one event per `requestAnimationFrame`. If still heavy, move motion to an unreliable
      channel + binary encoding (deferred).
- [ ] **Pointer button → focus / CSD.** Symptom: clicks don't focus, or GTK/Qt titlebar drag does
      nothing with a mouse. Cause: button-press must raise+focus the window under the pointer (that's
      what makes apps issue `move_request`/`resize_request`). Look: `input/pointer.rs` `pointer_button`
      (mirrors the canonical click-to-focus); `handlers/xdg_shell.rs` pointer `check_grab`.
- [ ] **pointerType routing.** Symptom: a touchpad/pen behaves like the wrong device. Cause: the
      `e.pointerType === "mouse"` split in `js/input_core.js`. Touchpads report `"mouse"` (correct —
      they emit wheel + pointer); pens report `"pen"` → touch path.
- [ ] **Keyboard repeat / placement / focus-follows.** These apply at **Start** (SessionConfig),
      not mid-session. Symptom: changing them does nothing until the next session. Look:
      `control.rs::start` (`change_repeat_info`, `placement`, `focus_follows_pointer`),
      `placement.rs` (Center/Cascade reposition on first commit — needs a real window size).
- [ ] **Show-touches overlay perf.** Symptom: overlay lag. It resizes a full-window canvas and
      clears per dot — fine for debug, not for high-rate use. Look: `js/overlay.js`.
- [ ] **Right-click vs move ambiguity.** Symptom: a long-press meant as right-click starts a move,
      or vice-versa. Both share the long-press entry; the split is move-on-drag / right-click-on-
      release-in-place. Look: `bridge.js` `held`→`move`/right-click transitions.
- [ ] **CSD touch grab serial match.** Symptom: dragging a GTK/Qt titlebar does nothing. Cause:
      `check_grab_touch` requires the app's `move_request` serial to match a live touch grab. Look:
      `handlers/xdg_shell.rs` `check_grab_touch`; `grabs/touch_{move,resize}_grab.rs`.
- [ ] **Multi-touch + global cancel caveat.** `wl_touch` cancel is global (all live contacts), so a
      long-press takeover (`CancelTouch`) cancels *every* active contact, not just the primary.
      Acceptable now; revisit if true multi-touch gestures need per-slot retraction.

## Deferred by scope — will resurface with later features (don't solve now, don't forget)

- [ ] **Two-finger / kinetic scroll** — touch-driven scroll (vs the wheel path we ship now); needs a
      real multi-touch client and momentum.
- [ ] **Pinch-zoom / rotate gestures** — multi-touch gesture recognition.
- [ ] **Unreliable-unordered motion channel + sequence numbers** — only needed once motion is
      high-rate enough that the reliable+ordered channel head-of-line-blocks.
- [ ] **Left/middle button emulation & touch DnD** — only right-click is emulated; DnD is still cancelled.
- [ ] **Soft-keyboard / IME composition** on phones (composed characters, autocorrect).

## Avoided by design (touch+keyboard-only, no cursor) — kept here so we remember why

- [x] **Cursor sync** — no cursor exists; `wl_touch`/`wl_keyboard` are cursorless.
- [x] **Pointer emulation** — not doing mouse; apps consume `wl_touch` directly.
- [x] **High-rate pointer-motion flooding** — no mouse motion stream this slice.
