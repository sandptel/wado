// wado bridge — the trackpad's touch surface. Turns pointers on one element into full-state
// frames: every finger down, every time (`{k:"frame", seq, t, c:[[id,x,y]…]}`), x/y normalised to
// 0..65535 over the element. No tap, scroll or palm logic here — the computer's own libinput
// reads the virtual touchpad these frames drive, which is the defence against misreads.
//
// When frames go out:
//   • a finger lands or lifts → at once (a tap must never be coalesced away)
//   • a finger moves → direct link: once per event-loop turn; relay: on a 16 ms tick
//   • a finger rests → re-sent every 100 ms, so the daemon's silence check (400 ms) only ever
//     lifts a finger whose phone is really gone
//
//   W.padTouch.attach(el) / detach()
//   W.padTouch.size() → {w, h} in CSS px;  W.padTouch.frame() → the current contacts
//   W.padTouch.resend()   size + current frame again (the link moved paths)

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
    const r = el.getBoundingClientRect();
    f.x = Math.round(clamp((e.clientX - r.left) / r.width) * 65535);
    f.y = Math.round(clamp((e.clientY - r.top) / r.height) * 65535);
  }
  function freeId() {
    const used = new Set([...fingers.values()].map((f) => f.id));
    for (let i = 0; i <= MAX_ID; i++) if (!used.has(i)) return i;
    return -1;
  }

  function down(e) {
    const id = freeId();
    if (id < 0) return;
    e.preventDefault();
    try { el.setPointerCapture(e.pointerId); } catch (_) {}
    const f = { id, x: 0, y: 0 };
    place(f, e);
    fingers.set(e.pointerId, f);
    soon();
  }
  function move(e) {
    const f = fingers.get(e.pointerId);
    if (!f) return;
    e.preventDefault();
    place(f, e);
    moved();
  }
  function up(e) {
    if (!fingers.delete(e.pointerId)) return;
    soon();
  }
  function hidden() {
    if (document.visibilityState === "hidden" && fingers.size) { fingers.clear(); send(); }
  }

  const on = [["pointerdown", down], ["pointermove", move], ["pointerup", up],
    ["pointercancel", up], ["lostpointercapture", up]];

  T.attach = (node) => {
    T.detach();
    el = node;
    for (const [ev, fn] of on) el.addEventListener(ev, fn);
    document.addEventListener("visibilitychange", hidden);
    const measure = () => {
      const r = el.getBoundingClientRect();
      const s = { w: Math.round(r.width), h: Math.round(r.height) };
      if (s.w !== size.w || s.h !== size.h) { size = s; sendSize(); }
    };
    if (typeof ResizeObserver !== "undefined") { ro = new ResizeObserver(measure); ro.observe(el); }
    measure();
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
    for (const t of timers) clearInterval(t);
    timers = [];
    if (fingers.size) { fingers.clear(); send(); }
    el = null;
    size = { w: 0, h: 0 };
  };
})();
