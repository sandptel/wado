// wado bridge — cursorless mouse pointer (pointerType "mouse"). Forwards real pointer
// features to the compositor without ever drawing a cursor: hover/motion (rAF-throttled),
// left/middle/right buttons (incl. real right-click), and wheel scroll. When Move-mode is on,
// a left-drag moves the window (WindowDrag) instead of a pointer drag.

const BTN_MAP = { 0: "left", 1: "middle", 2: "right" };

W.mouse = {
  _raf: null,
  _pending: null,
  _flushMotion() {
    W.mouse._raf = null;
    const p = W.mouse._pending;
    W.mouse._pending = null;
    if (p) W.sendInput({ t: "pointer_motion", x: p.x, y: p.y });
  },

  down(e, video) {
    const n = W.normPoint(e.clientX, e.clientY, video);
    if (!n) return;
    if (W.moveMode && e.button === 0) {
      W.mouseDragging = true;
      W.sendInput({ t: "window_drag", phase: "down", x: n.x, y: n.y });
      return;
    }
    const b = BTN_MAP[e.button];
    if (!b) return;
    W.sendInput({ t: "button", x: n.x, y: n.y, button: b, pressed: true });
  },

  move(e, video) {
    const n = W.normPoint(e.clientX, e.clientY, video);
    if (!n) return;
    if (W.showTouches && e.buttons) W.overlay.mark(e.clientX, e.clientY);
    if (W.mouseDragging) {
      W.sendInput({ t: "window_drag", phase: "motion", x: n.x, y: n.y });
      return;
    }
    // Coalesce hover/motion to one event per animation frame.
    W.mouse._pending = n;
    if (W.mouse._raf == null) W.mouse._raf = requestAnimationFrame(W.mouse._flushMotion);
  },

  up(e, video) {
    const n = W.normPoint(e.clientX, e.clientY, video);
    if (!n) return;
    if (W.mouseDragging) {
      W.mouseDragging = false;
      W.sendInput({ t: "window_drag", phase: "up", x: n.x, y: n.y });
      return;
    }
    const b = BTN_MAP[e.button];
    if (!b) return;
    W.sendInput({ t: "button", x: n.x, y: n.y, button: b, pressed: false });
  },

  wheel(e, video) {
    const n = W.normPoint(e.clientX, e.clientY, video);
    if (!n) return;
    // deltaMode 0=pixel, 1=line, 2=page → pixels; then apply speed + natural direction.
    const factor = e.deltaMode === 1 ? 16 : (e.deltaMode === 2 ? 100 : 1);
    const sign = W.naturalScroll ? -1 : 1;
    const speed = W.scrollSpeed || 1;
    W.sendInput({
      t: "scroll",
      x: n.x,
      y: n.y,
      dx: e.deltaX * factor * speed * sign,
      dy: e.deltaY * factor * speed * sign,
    });
  },
};
