// wado bridge — input capture & routing.
//
// Attaches DOM listeners to the <video> once and routes each event to the right subsystem,
// keyed on PointerEvent.pointerType: a real **mouse** drives the cursorless wl_pointer
// (W.mouse, see input_pointer.js); **touch/pen** drive wl_touch gestures (W.touchg, see
// input_touch.js); the keyboard goes to W.kbd (input_keyboard.js). Coordinates are normalized
// 0..1 against the displayed video *content* rect (object-fit:contain letterbox math) so the
// server scales 1:1 to the output. wado renders no cursor.

const INPUT_CHANNEL = "wado-input";   // must match wado_protocol::INPUT_CHANNEL
const MOTION_CHANNEL = "wado-motion"; // must match wado_protocol::MOTION_CHANNEL

// Event types that ride the zero-retransmit motion channel. Everything else
// goes reliable+ordered. Keep this list in sync with MOTION_CHANNEL's docs in the protocol
// crate: only absolute, latest-wins updates belong here — never a terminal or stateful
// event, which would be unrecoverable if dropped or harmful if reordered.
const MOTION_TYPES = new Set(["pointer_motion"]);
const isMotion = (obj) =>
  MOTION_TYPES.has(obj.t) || (obj.t === "window_drag" && obj.phase === "motion");
const HOLD_MS = 500;                // press-hold that promotes a touch contact to a gesture
const MOVE_THRESHOLD = 8;           // client-px movement that commits a touch to drag vs hold

// Send one input event on whichever channel suits its delivery needs. Falls back to the
// reliable channel if the motion channel isn't up yet — better a late motion than none.
W.sendInput = (obj) => {
  const motion = isMotion(obj);
  let dc = motion ? W.motionDC : W.inputDC;
  if (motion && !(dc && dc.readyState === "open")) dc = W.inputDC;
  if (dc && dc.readyState === "open") {
    try { dc.send(JSON.stringify(obj)); } catch (_) {}
    W.latency && W.latency.onInputSent && W.latency.onInputSent(obj);
  } else {
    console.warn("input dropped, channel not open:", dc && dc.readyState);
  }
};

// Normalize a client point to 0..1 within the video's rendered (letterboxed) content rect.
W.normPoint = (clientX, clientY, video) => {
  const r = video.getBoundingClientRect();
  const vw = video.videoWidth, vh = video.videoHeight;
  if (!vw || !vh || !r.width || !r.height) return null;
  const scale = Math.min(r.width / vw, r.height / vh); // object-fit: contain
  const cw = vw * scale, ch = vh * scale;
  const offX = r.left + (r.width - cw) / 2;
  const offY = r.top + (r.height - ch) / 2;
  const clamp = (v) => Math.min(1, Math.max(0, v));
  return { x: clamp((clientX - offX) / cw), y: clamp((clientY - offY) / ch) };
};

W.setupInputCapture = () => {
  if (W.inputCaptureReady) return;
  const video = document.getElementById("wado-video");
  if (!video) return;
  W.inputCaptureReady = true;
  W.videoEl = video;
  video.tabIndex = 0;
  video.style.touchAction = "none"; // stop browser pan/zoom so we get raw pointer events

  const isMouse = (e) => e.pointerType === "mouse";

  video.addEventListener("pointerdown", (e) => {
    e.preventDefault();
    video.focus();
    try { video.setPointerCapture(e.pointerId); } catch (_) {}
    W.activePointers.add(e.pointerId);
    if (W.showTouches) W.overlay.mark(e.clientX, e.clientY);
    (isMouse(e) ? W.mouse.down : W.touchg.down)(e, video);
  });
  video.addEventListener("pointermove", (e) => {
    // Mouse hover fires with no button down; touch only while a contact is held.
    if (!isMouse(e) && !W.activePointers.has(e.pointerId)) return;
    (isMouse(e) ? W.mouse.move : W.touchg.move)(e, video);
  });
  const end = (e) => {
    const had = W.activePointers.delete(e.pointerId);
    try { video.releasePointerCapture(e.pointerId); } catch (_) {}
    if (!isMouse(e) && !had) return;
    (isMouse(e) ? W.mouse.up : W.touchg.up)(e, video);
  };
  video.addEventListener("pointerup", end);
  video.addEventListener("pointercancel", end);

  // Mouse wheel → scroll (the only Wayland scroll path; no cursor drawn).
  video.addEventListener("wheel", (e) => { e.preventDefault(); W.mouse.wheel(e, video); }, { passive: false });
  // Let right-click reach the app, not the browser's context menu.
  video.addEventListener("contextmenu", (e) => e.preventDefault());

  // Keyboard at window level, forwarded only while the video is focused.
  window.addEventListener("keydown", (e) => W.kbd.down(e, video));
  window.addEventListener("keyup", (e) => W.kbd.up(e, video));
  video.addEventListener("blur", () => W.kbd.releaseAll());
};

// Drop all transient input state (called on session teardown).
W.resetInput = () => {
  W.coalesce.clear();
  // A lock outlives the session otherwise: the browser holds it against the video element,
  // which is still there, so the next session would start with the mouse already captured and
  // no obvious way to tell.
  if (W.pointerLock) W.pointerLock.release();
  if (W.gesture && W.gesture.holdTimer) clearTimeout(W.gesture.holdTimer);
  W.gesture = null;
  W.mouseDragging = false;
  W.pressedKeys.clear();
  W.activePointers.clear();
};
