// wado bridge — the gamepad's motion (spec §7): assemble on open, the press halo, a stick's thumb
// going home, the variant morph (FLIP), the editor's nudge, a look change popping in.
//
// Two curves only (`--spring` for spatial moves, `--pop` for what a finger just flipped), the
// duration tokens, transform / opacity / translate / scale only. Motion never sits on the send
// path: every press is sent in `pointerdown` before anything here is started. Levels follow
// `data-motion` and `prefers-reduced-motion`: full; reduced (short fades, no travel); off.
//
//   W.gpadMotion.level()              "full" | "reduced" | "off"
//   W.gpadMotion.assemble(map, o)     groups slide in from their nearest edge, in thumb order
//   W.gpadMotion.press(el)            the halo (Glow blooms, Ring holds, Still nothing)
//   W.gpadMotion.home(el, from)       a released thumb springs home on --pop
//   W.gpadMotion.rects(map)           → Map id → rect, before a change
//   W.gpadMotion.morph(box, first, ghosts, opts)  after the change: FLIP from `first`
//   W.gpadMotion.nudge(map)           entering the editor: one pop per control
//   W.gpadMotion.pop(els)             a look change: the faces pop in

(() => {
  const SPRING = "cubic-bezier(.32,.72,0,1)", POP = "cubic-bezier(.34,1.56,.64,1)", ACCEL = "cubic-bezier(.3,0,1,1)";
  const M = (W.gpadMotion = {});
  // Thumbs first, then the face, then shoulders, then the middle row.
  M.ORDER = ["stick-l", "dpad", "steer", "stick-r", "face", "pedals", "sh-l", "sh-r", "mid"];
  const gi = (el) => Math.max(0, M.ORDER.indexOf(el._c ? el._c.g : ""));

  M.level = () => {
    const m = document.documentElement.dataset.motion;
    if (m === "off") return "off";
    if (m === "reduced" || (m !== "full" && typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches)) return "reduced";
    return "full";
  };
  M.ms = (name, dflt) => {
    try {
      const v = parseFloat(getComputedStyle(document.documentElement).getPropertyValue(name));
      return isFinite(v) ? v : dflt;
    } catch (_) { return dflt; }
  };
  const anim = (el, kf, o) => (el && el.animate ? el.animate(kf, o) : null);

  M.assemble = (map, o) => {
    const m = M.level();
    if (m === "off") return;
    const st = M.ms("--stagger", 40), dur = M.ms("--dur-sheet", 480), base = M.ms("--dur", 220);
    const count = {};
    for (const el of map.values()) {
      if (m === "reduced") { anim(el, [{ opacity: 0 }, { opacity: 1 }], { duration: 160, fill: "backwards" }); continue; }
      const c = el._c, pos = c[o];
      const i = gi(el);
      // From the nearest edge: ≈ 35 px × the distance from centre.
      const dx = (pos[0] - 0.5) * 70, dy = (pos[1] - 0.5) * 50 + 14;
      anim(el, [{ transform: `translate(${dx}px, ${dy}px) scale(.82)`, opacity: 0 }, { transform: "none", opacity: 1 }],
        { duration: dur, delay: i * st, easing: SPRING, fill: "backwards" });
      if (c.g === "face") {
        const f = el.querySelector(".gpface");
        const n = (count[c.g] = (count[c.g] || 0) + 1) - 1;
        anim(f, [{ scale: 0.6 }, { scale: 1 }], { duration: base + 80, delay: i * st + 140 + n * 20, easing: POP, fill: "backwards" });
      }
    }
  };

  M.press = (el) => {
    if (M.level() !== "full") return;
    const press = el.dataset.press;
    if (press !== "glow") return;
    anim(el.querySelector(".gphalo"), [{ opacity: 0.6, scale: 0.92 }, { opacity: 0, scale: 1.35 }], { duration: 360, easing: SPRING });
  };

  M.home = (el, from) => {
    if (M.level() === "off" || !from) return;
    anim(el, [{ translate: from }, { translate: "0px 0px" }], { duration: M.ms("--dur", 220), easing: POP });
  };
  M.floatHome = (el, from) => {
    if (M.level() === "off" || !from) return;
    anim(el, [{ translate: from }, { translate: "0px 0px" }], { duration: M.ms("--dur-sheet", 480), easing: SPRING });
  };

  M.rects = (map) => {
    const out = new Map();
    for (const [id, el] of map) {
      out.set(id, el.getBoundingClientRect());
      if (el.getAnimations) for (const a of el.getAnimations()) { try { a.cancel(); } catch (_) {} }
    }
    return out;
  };

  // Ghosts: clones of controls that are leaving, so they fold away where they were.
  M.ghosts = (box, map, nextIds) => {
    const out = [];
    for (const [id, el] of map) {
      if (nextIds.has(id)) continue;
      const g = el.cloneNode(true);
      g.style.pointerEvents = "none";
      g.setAttribute("aria-hidden", "true");
      box.appendChild(g);
      out.push(g);
    }
    return out;
  };

  // `opts.dur`: a shorter morph (undo). Interruptible: a second call snapshots the visual rects.
  M.morph = (box, first, ghosts, opts = {}) => {
    const m = M.level(), map = box._gp || new Map();
    if (m !== "full") {
      for (const g of ghosts) g.remove();
      if (m === "reduced") for (const el of map.values()) anim(el, [{ opacity: 0 }, { opacity: 1 }], { duration: 160 });
      return;
    }
    const dur = opts.dur || M.ms("--dur-hero", 520), st = M.ms("--stagger", 40);
    for (const g of ghosts) {
      const a = anim(g, [{ opacity: 1, scale: 1 }, { opacity: 0, scale: 0.6 }], { duration: 180, easing: ACCEL, fill: "forwards" });
      if (a) a.onfinish = () => g.remove(); else g.remove();
    }
    for (const [id, el] of map) {
      const a = first.get(id), b = el.getBoundingClientRect();
      const i = gi(el);
      if (a && b.width && b.height) {
        const dx = a.left + a.width / 2 - (b.left + b.width / 2), dy = a.top + a.height / 2 - (b.top + b.height / 2);
        anim(el, [{ transform: `translate(${dx}px, ${dy}px) scale(${a.width / b.width}, ${a.height / b.height})` }, { transform: "none" }],
          { duration: dur, delay: opts.dur ? 0 : i * st * 0.6, easing: SPRING, fill: "backwards" });
        if (!opts.dur) anim(el.querySelector(".gpface"), [{ opacity: 0.35 }, { opacity: 1 }], { duration: 160, delay: 120, fill: "backwards" });
      } else {
        anim(el, [{ opacity: 0, scale: 0.6 }, { opacity: 1, scale: 1 }], { duration: 300, delay: 200 + i * st * 0.5, easing: POP, fill: "backwards" });
      }
    }
  };

  M.nudge = (map) => {
    if (M.level() !== "full") return;
    for (const el of map.values()) {
      anim(el, [{ scale: 1 }, { scale: 1.06 }, { scale: 1 }], { duration: M.ms("--dur", 220) + 100, delay: gi(el) * 20, easing: POP });
    }
  };

  M.pop = (els) => {
    if (M.level() !== "full") return;
    for (const el of els) anim(el.querySelector(".gpface, .gpdp"), [{ scale: 0.7, opacity: 0.4 }, { scale: 1, opacity: 1 }], { duration: 300, easing: POP });
  };
})();
