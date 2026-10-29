// wado bridge — the trackpad's touch surface. Turns pointers on one element into full-state
// frames: every finger down, every time (`{k:"frame", seq, t, c:[[id,x,y]…]}`), x/y normalised to
// 0..65535 over the element. No tap, scroll or palm logic here — the computer's own libinput
// reads the virtual touchpad these frames drive, which is the defence against misreads.
//
// When frames go out:
//   • a finger lands or lifts → at once (a tap must never be coalesced away); on the direct link,
//     whose touch channel never retransmits, the same frame goes again at +20 and +60 ms so one
//     lost packet cannot lose a click or hold a finger down (frames are whole state: repeats
//     are harmless)
//   • a finger moves → direct link: once per event-loop turn; relay: on a 16 ms tick
//   • a finger rests → re-sent every 100 ms, so the daemon's silence check (400 ms) only ever
//     lifts a finger whose phone is really gone
//
// Coordinates are the pad's own (js/pad_orient.js): when the pad is drawn turned for landscape,
// x/y and the size are the landscape pad's, so the daemon's pacer and libinput only ever see a
// landscape touchpad. Turning it mid-touch lifts every finger first, then sends the new size.
//
// Phone gestures (js/pad_gestures.js) stand in front: every pointer event asks them first, and
// they may hold an edge touch back ≤ 60 ms and replay it here (`feed`), or lift fingers they
// claimed (`drop`). Without an assigned gesture they pass everything straight through.
//
//   W.padTouch.attach(el) / detach()
//   W.padTouch.size() → {w, h} in CSS px;  W.padTouch.frame() → the current contacts
//   W.padTouch.resend()   size + current frame again (the link moved paths)
//   W.padTouch.feed("down" | "move" | "up", e)   an event the gestures held, now let through
//   W.padTouch.drop(pointerIds)                   lift these fingers on the computer now

(() => {
  const T = (W.padTouch = {});
  const MAX_ID = 9;
  let el = null;
  let ro = null;
  const fingers = new Map(); // pointerId → {id, x, y}
  let seq = 0;
  let dirty = false;
  let queued = false;
  let lastSent = 0;
  let timers = [];
  let size = { w: 0, h: 0 };
  let offTurn = null;

  T.frame = () => [...fingers.values()].sort((a, b) => a.id - b.id).map((f) => [f.id, f.x, f.y]);
  T.size = () => size;

  function send() {
    seq = (seq + 1) >>> 0;
    dirty = false;
    lastSent = performance.now();
    W.pad.send({ k: "frame", seq, t: (performance.now() | 0) >>> 0, c: T.frame() }, true);
  }
  function now() { queued = false; if (dirty) send(); }
  function soon() {
    dirty = true;
    if (!queued) { queued = true; queueMicrotask(now); }
  }
  // A finger landed or lifted.
  function changed() {
    soon();
    if (W.pad.state === "direct") for (const ms of [20, 60]) setTimeout(() => { if (el) send(); }, ms);
  }
  function moved() {
    dirty = true;
    if (W.pad.state === "direct") soon(); // relayed: the 16 ms tick sends it
  }

  function sendSize() {
    if (size.w && size.h) W.pad.send({ k: "size", w: size.w, h: size.h }, false);
  }
  T.resend = () => { sendSize(); send(); };

  const clamp = (v) => (v < 0 ? 0 : v > 1 ? 1 : v);
  function place(f, e) {
    const p = W.padOrient.toPad(e.clientX, e.clientY, el.getBoundingClientRect());
    f.x = Math.round(clamp(p.x / p.w) * 65535);
    f.y = Math.round(clamp(p.y / p.h) * 65535);
  }
  function freeId() {
    const used = new Set([...fingers.values()].map((f) => f.id));
    for (let i = 0; i <= MAX_ID; i++) if (!used.has(i)) return i;
    return -1;
  }

  function down(e) {
    const id = freeId();
    if (id < 0) return;
    const f = { id, x: 0, y: 0 };
    place(f, e);
    fingers.set(e.pointerId, f);
    changed();
  }
  function move(e) {
    const f = fingers.get(e.pointerId);
    if (!f) return;
    place(f, e);
    moved();
  }
  function up(e) {
    if (!fingers.delete(e.pointerId)) return;
    changed();
  }
  function hidden() {
    if (document.visibilityState === "hidden" && fingers.size) { fingers.clear(); send(); }
  }

  // Every event asks the phone gestures first; they say whether it is the pad's.
  const gate = (type, e) => !W.padGestures || W.padGestures.pass(type, e);
  function onDown(e) {
    e.preventDefault();
    try { el.setPointerCapture(e.pointerId); } catch (_) {}
    if (gate("down", e)) down(e);
  }
  function onMove(e) {
    e.preventDefault();
    if (gate("move", e)) move(e);
  }
  function onUp(e) { if (gate("up", e)) up(e); }

  T.feed = (type, e) => {
    if (!el) return;
    if (type === "down") { down(e); if (dirty) send(); } // its own frame, before anything after it
    else if (type === "move") move(e);
    else up(e);
  };
  T.drop = (ids) => {
    let any = false;
    for (const id of ids) any = fingers.delete(id) || any;
    if (any && el) changed();
  };

  const on = [["pointerdown", onDown], ["pointermove", onMove], ["pointerup", onUp],
    ["pointercancel", onUp], ["lostpointercapture", onUp]];

  T.attach = (node) => {
    T.detach();
    el = node;
    for (const [ev, fn] of on) el.addEventListener(ev, fn);
    document.addEventListener("visibilitychange", hidden);
    const measure = () => {
      const r = W.padOrient.size(el.getBoundingClientRect());
      const s = { w: Math.round(r.w), h: Math.round(r.h) };
      if (s.w !== size.w || s.h !== size.h) { size = s; sendSize(); }
    };
    if (typeof ResizeObserver !== "undefined") { ro = new ResizeObserver(measure); ro.observe(el); }
    measure();
    // Turned: the fingers down are in the old frame, so they lift here before the new size goes
    // (the daemon also lifts on a resize; a flip keeps the size, so this lift is what covers it).
    offTurn = W.padOrient.onChange(() => {
      if (fingers.size) { fingers.clear(); send(); }
      measure();
    });
    timers = [
      setInterval(() => { if (dirty && W.pad.state !== "direct") send(); }, 16),
      setInterval(() => { if (fingers.size && performance.now() - lastSent >= 100) send(); }, 50),
    ];
  };

  T.detach = () => {
    if (!el) return;
    for (const [ev, fn] of on) el.removeEventListener(ev, fn);
    document.removeEventListener("visibilitychange", hidden);
    if (ro) { ro.disconnect(); ro = null; }
    if (offTurn) { offTurn(); offTurn = null; }
    for (const t of timers) clearInterval(t);
    timers = [];
    if (fingers.size) { fingers.clear(); send(); }
    el = null;
    size = { w: 0, h: 0 };
  };
})();
