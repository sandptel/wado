// wado bridge — phone gestures: the few motions a laptop touchpad never makes, claimed for an
// action the user picked. It sits in front of js/pad_touch.js — js/pad_pointer.js asks
// `pass(type, p)` of every pointer first — and may hold back or swallow a pointer sequence only
// by these rules:
//
//   edge-left / edge-right  one finger landing within 10 px of that edge (as held) and moving
//                           inward ≥ 40 px within 300 ms. Only that first finger's touches are held,
//                           for at most 60 ms: then they are claimed, or replayed to the pad as the
//                           touch they were (one frame late by ≤ 60 ms, never more).
//   four-tap                four fingers down and all up within 250 ms, moving < 12 px. When it is
//                           assigned, the fourth finger is not forwarded and the three already on
//                           the laptop are held still; if it turns out to be a four-finger swipe
//                           they are handed back. The lift waits until ≥ 200 ms after the first
//                           finger landed, past libinput's tap timeout, so no three-finger tap fires.
//   two-hold                two fingers still (< 8 px) for 600 ms. They were forwarded all along
//                           (two resting fingers do nothing on a touchpad); on the claim they are
//                           lifted on the laptop (an empty frame), then the action runs.
//
// A gesture with no action assigned — or whose action this computer does not offer — is never
// claimed and nothing is held: the touch goes through exactly as without this file. Two- and
// three-finger swipes and pinches are never claimed.
//
//   W.padGestures.attach(el) / detach()
//   W.padGestures.pass("down" | "move" | "up", p) → true: let js/pad_touch.js have it
//                                                    p = {id, x, y, w, h} in the pad's own px
//   W.padGestures.onDone(fn(action, {ok, message}))

(() => {
  const G = (W.padGestures = {});
  const EDGE_PX = 10;
  const EDGE_IN_PX = 40;
  const EDGE_MS = 300;
  const HOLD_BACK_MS = 60;
  const EDGE_LEAN_PX = 6; // at the 60 ms decision, this much inward motion (and more in than along) claims
  const FOUR_MS = 250;
  const FOUR_STILL_PX = 12;
  const TAP_LIFT_MS = 200; // libinput's tap timeout is 180 ms: lift held fingers no sooner than this
  const TWO_MS = 600;
  const TWO_STILL_PX = 8;

  let el = null;
  const fingers = new Map(); // pointerId → {x0, y0, x, y, p}
  let mode = null; // null | "edge-hold" | "edge" | "four" | "two" — the claim on this sequence
  let start = 0; // the first finger of this sequence landed
  let edge = null; // {side, id, held: [[type, p]…], fired, timer}
  let four = null; // {fourth, frozen: [ids]}
  let twoTimer = 0;
  const doneFns = [];
  G.onDone = (fn) => doneFns.push(fn);

  const now = () => performance.now();
  function actionFor(name) {
    const id = W.padPrefs.get().gestures[name];
    return id ? W.padActions.byId(id) : null;
  }
  const dist = (f) => Math.hypot(f.x - f.x0, f.y - f.y0);
  const feed = (type, p) => W.padTouch.feed(type, p);

  // ── visuals: an accent glow on the edge, a pulse where a claim happened ─────────────────
  let glow = null;
  function showGlow(side, y, progress) {
    if (!el.appendChild) return;
    if (!glow) { glow = document.createElement("div"); glow.className = "tpedge"; el.appendChild(glow); }
    glow.dataset.side = side;
    glow.style.setProperty("--p", String(Math.min(1, progress)));
    glow.style.transform = `translateY(${y}px)`;
    glow.classList.add("on");
  }
  function hideGlow() { if (glow) glow.classList.remove("on", "fired"); }
  function pulse(action, x, y) {
    if (!el.appendChild) return;
    const n = document.createElement("div");
    n.className = "tppulse";
    n.innerHTML = W.padIcon(action.icon);
    n.style.transform = `translate(${x}px, ${y}px)`;
    el.appendChild(n);
    setTimeout(() => n.remove(), 320);
  }

  async function fire(action, x, y) {
    pulse(action, x, y);
    if (W.native && W.native.haptic) W.native.haptic("claim"); // the Android app (native_feel.js)
    const res = await W.padActions.run(action.id);
    for (const fn of doneFns) { try { fn(action, res); } catch (_) {} }
  }

  // ── edge ─────────────────────────────────────────────────────────────────────────────
  const inward = (f) => (edge.side === "edge-left" ? f.x - f.x0 : f.x0 - f.x);

  function edgeRelease() {
    // Not an edge swipe: the held touches go to the pad as they happened, the move a beat later
    // so the first frame carries where the finger landed.
    const held = edge.held;
    clearTimeout(edge.timer);
    edge = null;
    mode = null;
    hideGlow();
    const down = held.find(([t]) => t === "down");
    const lastMove = [...held].reverse().find(([t]) => t === "move");
    if (down) feed("down", down[1]);
    if (lastMove) queueMicrotask(() => feed("move", lastMove[1]));
  }

  function edgeClaim() {
    clearTimeout(edge.timer);
    mode = "edge";
  }

  function edgeCheck(f) {
    const p = inward(f) / EDGE_IN_PX;
    showGlow(edge.side, f.y, Math.max(0, p));
    if (!edge.fired && inward(f) >= EDGE_IN_PX && now() - start <= EDGE_MS) {
      edge.fired = true;
      if (mode === "edge-hold") edgeClaim();
      if (glow) glow.classList.add("fired");
      fire(edge.action, f.x, f.y);
    }
  }

  function edgeDecide() {
    if (mode !== "edge-hold" || !edge) return;
    const f = fingers.get(edge.id);
    const lean = f ? inward(f) : 0;
    if (f && lean >= EDGE_LEAN_PX && lean > Math.abs(f.y - f.y0)) edgeClaim();
    else edgeRelease();
  }

  // ── four ─────────────────────────────────────────────────────────────────────────────
  function fourHandBack() {
    // A four-finger swipe after all: the held fingers move to where they are, the fourth lands.
    const f4 = four;
    four = null;
    mode = null;
    W.padTouch.drop(f4.frozen.filter((id) => !fingers.has(id))); // lifted while held
    for (const id of f4.frozen) { const f = fingers.get(id); if (f) feed("move", f.p); }
    const f = fingers.get(f4.fourth);
    if (f) feed("down", f.p);
  }

  function fourEnd() {
    const f4 = four;
    const tap = now() - start <= FOUR_MS && !f4.moved;
    const lift = () => W.padTouch.drop(f4.frozen);
    const wait = start + TAP_LIFT_MS - now();
    if (wait > 0) setTimeout(lift, wait); else lift();
    four = null;
    if (tap) fire(f4.action, f4.x, f4.y);
  }

  // ── two ──────────────────────────────────────────────────────────────────────────────
  function twoCancel() { clearTimeout(twoTimer); twoTimer = 0; }
  function twoFire() {
    twoTimer = 0;
    const action = actionFor("two-hold");
    if (!action || fingers.size !== 2 || mode) return;
    const fs = [...fingers.values()];
    if (fs.some((f) => dist(f) >= TWO_STILL_PX)) return;
    mode = "two";
    W.padTouch.drop([...fingers.keys()]); // lift them on the laptop first
    fire(action, (fs[0].x + fs[1].x) / 2, (fs[0].y + fs[1].y) / 2);
  }

  // ── the gate ─────────────────────────────────────────────────────────────────────────
  function down(p) {
    const f = { x0: p.x, y0: p.y, x: p.x, y: p.y, p };
    const first = fingers.size === 0;
    if (first) { start = now(); mode = null; }
    fingers.set(p.id, f);

    if (mode === "edge-hold") { edgeRelease(); return true; } // a second finger: no edge swipe
    if (mode) return false; // claimed: later fingers are part of it

    if (first) {
      const side = p.x < EDGE_PX ? "edge-left" : p.x > p.w - EDGE_PX ? "edge-right" : null;
      const action = side && actionFor(side);
      if (action) {
        mode = "edge-hold";
        edge = { side, id: p.id, held: [["down", p]], fired: false, action, timer: setTimeout(edgeDecide, HOLD_BACK_MS) };
        return false;
      }
    }

    if (fingers.size === 4 && now() - start <= FOUR_MS) {
      const action = actionFor("four-tap");
      if (action && [...fingers.values()].every((g) => dist(g) < FOUR_STILL_PX)) {
        mode = "four";
        const others = [...fingers.keys()].filter((id) => id !== p.id);
        four = { fourth: p.id, frozen: others, action, moved: false, x: 0, y: 0 };
        const fs = [...fingers.values()];
        four.x = fs.reduce((s, g) => s + g.x, 0) / 4;
        four.y = fs.reduce((s, g) => s + g.y, 0) / 4;
        twoCancel();
        return false;
      }
    }

    twoCancel();
    if (fingers.size === 2 && actionFor("two-hold")) twoTimer = setTimeout(twoFire, TWO_MS);
    return true;
  }

  function move(p) {
    const f = fingers.get(p.id);
    if (!f) return true;
    f.x = p.x; f.y = p.y; f.p = p;
    if (mode === "edge-hold") {
      if (p.id === edge.id) { edge.held.push(["move", p]); edgeCheck(f); }
      return false;
    }
    if (mode === "edge") { if (edge && p.id === edge.id) edgeCheck(f); return false; }
    if (mode === "four") {
      if (dist(f) >= FOUR_STILL_PX) { four.moved = true; fourHandBack(); return true; }
      return false;
    }
    if (mode) return false;
    if (twoTimer && dist(f) >= TWO_STILL_PX) twoCancel(); // a scroll, not a hold
    return true;
  }

  function up(p) {
    if (!fingers.has(p.id)) return true;
    const was = mode;
    let pass = true;
    if (was === "edge-hold") {
      if (p.id === edge.id) edgeRelease(); // a tap at the edge: forwarded as a tap
    } else if (was === "four") {
      pass = false;
    } else if (was) {
      pass = false;
    } else {
      twoCancel();
    }
    fingers.delete(p.id);
    if (!fingers.size) {
      if (was === "four" && four) fourEnd();
      if (edge) { clearTimeout(edge.timer); edge = null; }
      hideGlow();
      twoCancel();
      mode = null;
    }
    return pass;
  }

  G.pass = (type, p) => {
    if (!el) return true;
    if (type === "down") return down(p);
    if (type === "move") return move(p);
    return up(p);
  };

  G.attach = (node) => {
    G.detach();
    el = node;
  };

  G.detach = () => {
    if (edge) clearTimeout(edge.timer);
    twoCancel();
    fingers.clear();
    edge = four = null;
    mode = null;
    if (glow) { glow.remove(); glow = null; }
    el = null;
  };
})();
