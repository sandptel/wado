// wado bridge — gesture hints: the pad says what it thinks the fingers are doing, as they do it.
// Two fingers moving together get scroll arrows trailing their centre; two fingers spreading or
// closing get pinch arrows and the live scale; three or four fingers sliding get a swipe arrow;
// four fingers down get their own label; one finger held still fills a ring.
//
// Purely a picture. It is handed the same pointers as js/pad_touch.js, after the send, by
// js/pad_pointer.js; it sends nothing and delays nothing: libinput on the computer decides what
// a gesture is, and these hints only mirror the obvious cases. Positions are the pad's own frame
// (js/pad_orient.js) and written once per animation frame, transform/opacity only.
//
//   W.padHints.attach(el) / detach()
//   W.padHints.input("down" | "move" | "up", p)   p = {id, x, y, w, h} in the pad's own px
//   W.padHints.classify(fingers, ms) → {kind, …} | null   fingers: [{x0, y0, x, y}], ms since
//                                                         this set of fingers came down

(() => {
  const H = (W.padHints = {});
  const HOLD_MS = 350; // a press this long and this still is a hold…
  const HOLD_FILL_MS = 550; // …and the ring fills over this much more
  const STILL_PX = 8;
  const PINCH_PX = 24;
  const SCROLL_PX = 16;
  const SWIPE_PX = 24;

  const mid = (pts, k) => {
    const x = pts.reduce((s, p) => s + p[k + "x"], 0) / pts.length;
    const y = pts.reduce((s, p) => s + p[k + "y"], 0) / pts.length;
    return { x, y };
  };
  const axisOf = (dx, dy) => (Math.abs(dx) >= Math.abs(dy) ? "h" : "v");

  H.classify = (f, ms) => {
    const n = f.length;
    if (!n || n > 4) return null;
    const c0 = { x: f.reduce((s, p) => s + p.x0, 0) / n, y: f.reduce((s, p) => s + p.y0, 0) / n };
    const c = mid(f, "");
    const dx = c.x - c0.x, dy = c.y - c0.y;
    const moved = Math.hypot(dx, dy);
    if (n === 1) {
      if (moved >= STILL_PX || ms < HOLD_MS) return null;
      return { kind: "hold", progress: Math.min(1, (ms - HOLD_MS) / HOLD_FILL_MS), x: c.x, y: c.y };
    }
    // How far the fingers reach ahead of and behind their centre along the way they move, so the
    // arrows can lead past the leading finger and the label can trail behind the last.
    const u = moved ? { x: dx / moved, y: dy / moved } : { x: 1, y: 0 };
    const along = f.map((p) => (p.x - c.x) * u.x + (p.y - c.y) * u.y);
    const ahead = Math.max(0, ...along), behind = Math.max(0, ...along.map((a) => -a));
    if (n === 2) {
      const d0 = Math.hypot(f[1].x0 - f[0].x0, f[1].y0 - f[0].y0);
      const d = Math.hypot(f[1].x - f[0].x, f[1].y - f[0].y);
      const dd = d - d0;
      if (Math.abs(dd) > PINCH_PX && Math.abs(dd) > moved) {
        const angle = Math.atan2(f[1].y - f[0].y, f[1].x - f[0].x);
        return { kind: "pinch", out: dd > 0, scale: d0 ? d / d0 : 1, angle, half: d / 2, x: c.x, y: c.y };
      }
      if (moved > SCROLL_PX) return { kind: "scroll", axis: axisOf(dx, dy), dx, dy, ahead, behind, x: c.x, y: c.y };
      return null;
    }
    if (moved > SWIPE_PX) return { kind: "swipe", n, axis: axisOf(dx, dy), dx, dy, ahead, behind, x: c.x, y: c.y };
    return n === 4 ? { kind: "four", x: c.x, y: c.y } : null;
  };

  let el = null;
  let g = null; // the hint element, built once per attach
  const fingers = new Map(); // pointerId → {x0, y0, x, y}
  let since = 0;
  let locked = null; // once a two-finger set reads as pinch or scroll, it stays that
  let solo = false; // one finger that came down alone (not the last of a set) may read as a hold
  let raf = 0;
  let tick = 0;

  const CHEV = "M9 6l6 6-6 6";
  const svg = (d) => `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="${d}"/></svg>`;

  function build() {
    g = document.createElement("div");
    g.className = "tpg";
    g.innerHTML =
      `<div class="tpg-flow">${svg(CHEV)}${svg(CHEV)}${svg(CHEV)}</div>` +
      `<div class="tpg-pinch"><span class="tpg-a">${svg(CHEV)}</span><span class="tpg-b">${svg(CHEV)}</span></div>` +
      `<div class="tpg-hold"><i></i></div>` +
      `<div class="tpg-label"></div>`;
    el.appendChild(g);
  }

  function place(f, p) {
    f.x = p.x;
    f.y = p.y;
  }
  // A finger came or went: the gesture starts over from where every finger now is.
  function restart() {
    since = performance.now();
    locked = null;
    for (const f of fingers.values()) { f.x0 = f.x; f.y0 = f.y; }
  }

  const LABEL = {
    scroll: () => "Scroll",
    pinch: (h) => `Pinch ×${h.scale.toFixed(1)}`,
    swipe: (h) => (h.n === 3 && h.axis === "h" ? "Workspace" : h.n === 3 ? "Three-finger swipe" : "Four-finger swipe"),
    four: () => "Four fingers",
    hold: (h) => (h.progress >= 1 ? "Holding" : ""),
  };

  function paint() {
    raf = 0;
    if (!el || !g) return;
    let h = fingers.size ? H.classify([...fingers.values()], performance.now() - since) : null;
    if (fingers.size === 2) {
      if (h && (h.kind === "pinch" || h.kind === "scroll")) locked = locked || h.kind;
      if (locked && (!h || h.kind !== locked)) h = null; // keep it steady rather than flicker
    }
    if (h && h.kind === "hold" && !solo) h = null;
    if (!h) { g.classList.remove("on"); return; }
    g.dataset.k = h.kind;
    g.style.transform = `translate(${h.x}px, ${h.y}px)`;
    const label = LABEL[h.kind](h);
    const lab = g.querySelector(".tpg-label");
    if (lab.textContent !== label) lab.textContent = label;
    lab.hidden = !label;
    let off = { x: 0, y: 64 }; // the label: under the fingers, unless they are moving
    if (h.kind === "scroll" || h.kind === "swipe") {
      const a = Math.atan2(h.dy, h.dx);
      // Arrows lead past the leading finger's ring; the label trails behind the last one.
      g.querySelector(".tpg-flow").style.transform =
        `rotate(${(a * 180) / Math.PI}deg) translateX(${h.ahead + 30}px)`;
      off = { x: -Math.cos(a) * (h.behind + 84), y: -Math.sin(a) * (h.behind + 50) };
    } else if (h.kind === "pinch") {
      const deg = (h.angle * 180) / Math.PI;
      const out = h.out ? 0 : 180;
      g.querySelector(".tpg-pinch").style.transform = `rotate(${deg}deg)`;
      g.querySelector(".tpg-a").style.transform = `translateX(${-h.half - 44}px) rotate(${180 + out}deg)`;
      g.querySelector(".tpg-b").style.transform = `translateX(${h.half + 44}px) rotate(${out}deg)`;
      off = { x: -Math.sin(h.angle) * 60, y: Math.cos(h.angle) * 60 }; // beside the pinch's line
    } else if (h.kind === "hold") {
      g.querySelector(".tpg-hold i").style.transform = `scale(${0.25 + 0.75 * h.progress})`;
      g.classList.toggle("full", h.progress >= 1);
      if (h.progress < 1) schedule();
    }
    lab.style.transform = `translate(${off.x}px, ${off.y}px)`;
    g.classList.add("on");
  }
  function schedule() { if (!raf) raf = requestAnimationFrame(paint); }

  function down(p) {
    const f = { x: 0, y: 0, x0: 0, y0: 0 };
    place(f, p);
    fingers.set(p.id, f);
    restart();
    solo = fingers.size === 1;
    // A held finger fills its ring even while nothing moves.
    clearTimeout(tick);
    if (fingers.size === 1) tick = setTimeout(schedule, HOLD_MS);
    schedule();
  }
  function move(p) {
    const f = fingers.get(p.id);
    if (!f) return;
    place(f, p);
    schedule();
  }
  function up(p) {
    if (!fingers.delete(p.id)) return;
    solo = false;
    restart();
    schedule();
  }

  H.input = (type, p) => {
    if (!el) return;
    if (type === "down") down(p);
    else if (type === "move") move(p);
    else up(p);
  };

  H.attach = (node) => {
    H.detach();
    el = node;
    build();
  };

  H.detach = () => {
    if (!el) return;
    if (raf) { cancelAnimationFrame(raf); raf = 0; }
    clearTimeout(tick);
    fingers.clear();
    if (g) { g.remove(); g = null; }
    el = null;
  };
})();
