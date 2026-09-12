// wado bridge — cursorless mouse pointer (pointerType "mouse"). Forwards real pointer
// features to the compositor without ever drawing a cursor: hover/motion (rAF-throttled),
// left/middle/right buttons (incl. real right-click), and wheel scroll. When Move-mode is on,
// a left-drag moves the window (WindowDrag) instead of a pointer drag.

const BTN_MAP = { 0: "left", 1: "middle", 2: "right" };

W.mouse = {
  down(e, video) {
    const n = W.normPoint(e.clientX, e.clientY, video);
    if (!n) return;
    if (W.moveMode && e.button === 0) {
      W.mouseDragging = true;
      W.coalesce.now({ t: "window_drag", phase: "down", x: n.x, y: n.y });
      return;
    }
    const b = BTN_MAP[e.button];
    if (!b) return;
    W.coalesce.now({ t: "button", x: n.x, y: n.y, button: b, pressed: true });
  },

  move(e, video) {
    const n = W.normPoint(e.clientX, e.clientY, video);
    if (!n) return;
    if (W.showTouches && e.buttons) W.overlay.mark(e.clientX, e.clientY);
    // Both branches coalesce. Drag motion used to send on every `pointermove`, which at
    // 1000 Hz saturated the input channel and made dragging lag further behind the longer
    // it went on — the one path that most needed rate-limiting was the one that lacked it.
    if (W.mouseDragging) {
      W.coalesce.queue("window_drag", { t: "window_drag", phase: "motion", x: n.x, y: n.y });
      return;
    }
    W.coalesce.queue("pointer_motion", { t: "pointer_motion", x: n.x, y: n.y });
  },

  up(e, video) {
    const n = W.normPoint(e.clientX, e.clientY, video);
    if (!n) return;
    if (W.mouseDragging) {
      W.mouseDragging = false;
      // `now` flushes the queued motion first, so the release can't overtake it and snap
      // the window back to a stale position.
      W.coalesce.now({ t: "window_drag", phase: "up", x: n.x, y: n.y });
      return;
    }
    const b = BTN_MAP[e.button];
    if (!b) return;
    W.coalesce.now({ t: "button", x: n.x, y: n.y, button: b, pressed: false });
  },

  wheel(e, video) {
    const n = W.normPoint(e.clientX, e.clientY, video);
    if (!n) return;
    // deltaMode 0=pixel, 1=line, 2=page → pixels; then apply speed + natural direction.
    const factor = e.deltaMode === 1 ? 16 : (e.deltaMode === 2 ? 100 : 1);
    const sign = W.naturalScroll ? -1 : 1;
    // Pixel-mode deltas are what a touchpad sends (many small ones), so they get the same
    // velocity gain as a finger drag. Line and page modes are a real wheel's discrete
    // notches: accelerating those makes a mouse feel broken, so they stay linear.
    const accel = e.deltaMode === 0 ? W.scrollAccel.gain(Math.hypot(e.deltaX, e.deltaY)) : 1;
    // Same viewer-to-session conversion as the finger drag (input_units.js). A wheel notch is
    // also measured in the viewer's pixels, so it under-scrolled by exactly the same factor.
    const speed = (W.scrollSpeed || 1) * accel * W.cssToLogical(video);
    W.sendInput({
      t: "scroll",
      x: n.x,
      y: n.y,
      dx: e.deltaX * factor * speed * sign,
      dy: e.deltaY * factor * speed * sign,
    });
  },
};
