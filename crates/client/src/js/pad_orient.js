// wado bridge — which way round the trackpad is drawn, and the maths that goes with it.
//
// Landscape is how the pad is meant to be held. With the phone's rotation lock on, the browser
// stays upright, so the pad draws itself turned 90° (`flip` picks which way) at the viewport's
// height × width, and every pointer is turned back into the pad's own frame before it is sent:
// a finger moving right *as the phone is held* is +x on the pad, whatever the screen thinks.
// When the screen is already landscape nothing is turned. Fullscreen asks for the real thing —
// `requestFullscreen()` then `screen.orientation.lock("landscape")` — and falls back to drawing
// it turned, saying so once.
//
// Typing un-turns it: the phone's own keyboard always comes up upright, so while the keyboard
// is open the whole pad is upright with it (layout A), and turns back when it closes.
//
//   W.padOrient.attach(root) / detach()
//   W.padOrient.update({landscape, flip, fullscreen, typing})   any subset
//   W.padOrient.angle      0 | 90 | -90 — how far the pad is drawn turned from the screen
//   W.padOrient.layout     "portrait" | "landscape"
//   W.padOrient.toPad(clientX, clientY, rect) → {x, y, w, h} in the pad's own px
//   W.padOrient.size(rect) → {w, h}   the pad's own size (rect: its on-screen bounding box)
//   W.padOrient.onChange(fn) → off()   after the angle or layout changed
//   W.padOrient.onNote(fn)             fn(text): a one-off quiet note (fullscreen fell back)
//   W.padOrient.rotate(angle)          set the angle directly (checks)

(() => {
  const O = (W.padOrient = { angle: 0, layout: "portrait" });
  const changeFns = new Set();
  const noteFns = [];
  let root = null;
  let want = { landscape: false, flip: false, fullscreen: false, typing: false };
  let wentFull = false; // we entered fullscreen, so we leave it
  let noted = false;

  O.onChange = (fn) => { changeFns.add(fn); return () => changeFns.delete(fn); };
  O.onNote = (fn) => noteFns.push(fn);
  const note = (s) => { if (noted) return; noted = true; for (const fn of noteFns) { try { fn(s); } catch (_) {} } };

  // The pad's own frame from a pointer on screen. `rect` is the pad's on-screen bounding box,
  // which for a turned pad is upright and the other way round. Turned 90° clockwise, the pad's
  // x axis runs down the screen and its y axis runs right-to-left; turned −90°, the reverse.
  O.toPad = (cx, cy, r) => {
    const right = r.left + r.width, bottom = r.top + r.height;
    if (O.angle === 90) return { x: cy - r.top, y: right - cx, w: r.height, h: r.width };
    if (O.angle === -90) return { x: bottom - cy, y: cx - r.left, w: r.height, h: r.width };
    return { x: cx - r.left, y: cy - r.top, w: r.width, h: r.height };
  };
  O.size = (r) => (O.angle ? { w: r.height, h: r.width } : { w: r.width, h: r.height });

  function set(angle, layout) {
    if (angle === O.angle && layout === O.layout) return;
    O.angle = angle;
    O.layout = layout;
    if (root) {
      if (angle) root.setAttribute("data-rot", String(angle)); else root.removeAttribute("data-rot");
      root.setAttribute("data-layout", layout);
    }
    for (const fn of [...changeFns]) { try { fn(O.angle, O.layout); } catch (_) {} }
  }
  O.rotate = (angle) => set(angle, angle ? "landscape" : O.layout);

  // Read the viewport, never `screen.orientation`: headless and some phones report the screen
  // as landscape while the page is upright.
  const upright = () => typeof innerWidth !== "number" || innerHeight >= innerWidth;

  function compute() {
    if (!root) return;
    if (want.typing || !want.landscape) return set(0, "portrait");
    if (!upright()) return set(0, "landscape");
    set(want.flip ? -90 : 90, "landscape");
  }

  async function goFull() {
    if (wentFull || !root) return;
    const de = document.documentElement;
    try {
      if (!document.fullscreenElement) {
        if (!de.requestFullscreen) throw new Error("no fullscreen");
        await de.requestFullscreen({ navigationUI: "hide" });
      }
      wentFull = true;
      if (!screen.orientation || !screen.orientation.lock) throw new Error("no orientation lock");
      await screen.orientation.lock("landscape");
    } catch (_) {
      note("Full screen isn't available here, so the pad is drawn sideways instead.");
    }
    compute();
  }
  function leaveFull() {
    if (!wentFull) return;
    wentFull = false;
    try { screen.orientation.unlock(); } catch (_) {}
    if (document.fullscreenElement && document.exitFullscreen) document.exitFullscreen().catch(() => {});
  }

  O.update = (patch) => {
    want = { ...want, ...patch };
    if (root && want.landscape && want.fullscreen) goFull();
    else leaveFull();
    compute();
  };

  let raf = 0;
  const onResize = () => { if (!raf) raf = requestAnimationFrame(() => { raf = 0; compute(); }); };

  O.attach = (node) => {
    O.detach();
    root = node;
    O.angle = 0;
    O.layout = "";
    addEventListener("resize", onResize);
    addEventListener("orientationchange", onResize);
    compute();
    if (want.landscape && want.fullscreen) goFull();
  };

  O.detach = () => {
    if (!root) return;
    removeEventListener("resize", onResize);
    removeEventListener("orientationchange", onResize);
    if (raf) { cancelAnimationFrame(raf); raf = 0; }
    leaveFull();
    root.removeAttribute("data-rot");
    root.removeAttribute("data-layout");
    root = null;
    O.angle = 0;
    O.layout = "portrait";
  };
})();
