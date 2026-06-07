// wado bridge — input capture & routing.
//
// Attaches DOM listeners to the <video> once and routes each event to the right subsystem,
// keyed on PointerEvent.pointerType: a real **mouse** drives the cursorless wl_pointer
// (W.mouse, see input_pointer.js); **touch/pen** drive wl_touch gestures (W.touchg, see
// input_touch.js); the keyboard goes to W.kbd (input_keyboard.js). Coordinates are normalized
// 0..1 against the displayed video *content* rect (object-fit:contain letterbox math) so the
// server scales 1:1 to the output. wado renders no cursor.

const INPUT_CHANNEL = "wado-input"; // must match wado_protocol::INPUT_CHANNEL
const HOLD_MS = 500;                // press-hold that promotes a touch contact to a gesture
const MOVE_THRESHOLD = 8;           // client-px movement that commits a touch to drag vs hold

W.sendInput = (obj) => {
  const dc = W.inputDC;
  if (dc && dc.readyState === "open") {
    try { dc.send(JSON.stringify(obj)); } catch (_) {}
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
  if (W.gesture && W.gesture.holdTimer) clearTimeout(W.gesture.holdTimer);
  W.gesture = null;
  W.mouseDragging = false;
  W.pressedKeys.clear();
  W.activePointers.clear();
};
