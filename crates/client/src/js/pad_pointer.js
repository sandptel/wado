// wado bridge — the trackpad's one pointer pipeline. Owns the pad's pointer listeners, so each
// pointer event is read once, mapped once into the pad's own frame (js/pad_orient.js) and handed
// on in a fixed order:
//
//   1. phone gestures (js/pad_gestures.js) — may hold the event back or claim it
//   2. touch (js/pad_touch.js) — the send path; it queues its frame in this same event turn
//   3. rings (js/pad_feel.js) and hints (js/pad_hints.js) — pictures only, given the event in a
//      microtask queued after the send's, so drawing never runs ahead of what the laptop gets.
//      They paint on the next animation frame. A failure in one of them stays there.
//
// The pad's on-screen box (`getBoundingClientRect`, a forced layout) is read at most once per
// animation frame: kept until the next frame, or until something that moves it — the pad
// resizing, turning, the window or the visual viewport changing, the page coming back.
//
// A point is the pad's own px: {id: pointerId, x, y, w, h}.
//
//   W.padPointer.attach(el) / detach()
//   W.padPointer.rect()   the pad's on-screen box, as read this frame

(() => {
  const P = (W.padPointer = {});
  let el = null;
  let box = null; // this frame's read, until invalidated
  let raf = 0;
  let ro = null;
  let offTurn = null;

  function invalidate() { box = null; }
  function nextFrame() { raf = 0; box = null; }

  P.rect = () => {
    if (!box) {
      box = el.getBoundingClientRect();
      if (!raf) raf = requestAnimationFrame(nextFrame);
    }
    return box;
  };

  function point(e) {
    const p = W.padOrient.toPad(e.clientX, e.clientY, P.rect());
    p.id = e.pointerId;
    return p;
  }

  function route(type, e) {
    const p = point(e);
    if (!W.padGestures || W.padGestures.pass(type, p)) W.padTouch.input(type, p);
    queueMicrotask(() => {
      for (const view of [W.padFeel, W.padHints]) {
        if (!view) continue;
        try { view.input(type, p); } catch (err) { console.error(err); }
      }
    });
  }

  function onDown(e) {
    e.preventDefault();
    try { el.setPointerCapture(e.pointerId); } catch (_) {}
    route("down", e);
  }
  function onMove(e) {
    e.preventDefault();
    route("move", e);
  }
  function onUp(e) { route("up", e); }
  function onVisible() { if (document.visibilityState === "visible") invalidate(); }

  const on = [["pointerdown", onDown], ["pointermove", onMove], ["pointerup", onUp],
    ["pointercancel", onUp], ["lostpointercapture", onUp]];
  const vv = () => (typeof visualViewport !== "undefined" ? visualViewport : null);

  P.attach = (node) => {
    if (el === node) return;
    P.detach();
    el = node;
    for (const [ev, fn] of on) el.addEventListener(ev, fn);
    if (typeof ResizeObserver !== "undefined") { ro = new ResizeObserver(invalidate); ro.observe(el); }
    offTurn = W.padOrient.onChange(invalidate);
    addEventListener("resize", invalidate);
    if (vv()) { vv().addEventListener("resize", invalidate); vv().addEventListener("scroll", invalidate); }
    document.addEventListener("visibilitychange", onVisible);
  };

  P.detach = () => {
    if (!el) return;
    for (const [ev, fn] of on) el.removeEventListener(ev, fn);
    if (ro) { ro.disconnect(); ro = null; }
    if (offTurn) { offTurn(); offTurn = null; }
    removeEventListener("resize", invalidate);
    if (vv()) { vv().removeEventListener("resize", invalidate); vv().removeEventListener("scroll", invalidate); }
    document.removeEventListener("visibilitychange", onVisible);
    if (raf) { cancelAnimationFrame(raf); raf = 0; }
    box = null;
    el = null;
  };
})();
