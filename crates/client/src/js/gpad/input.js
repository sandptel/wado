// wado bridge — fingers on a rendered pad (`gpad/render.js`) become controller events
// (`gpad/controller.js`): buttons, the d-pad (4- or 8-way), sticks (fixed / floating / relative,
// with a dead zone), analog pedals and the steering slider (or the phone's tilt).
//
// The send happens first, in `pointerdown` / `pointermove`; the look of the press (classes, the
// thumb under the finger 1:1, trails, the direction nub, the halo) comes after it (§7). Every
// pointer is captured by its control, so a thumb sliding off a button still releases it, and
// `pointercancel` / `lostpointercapture` release too: a missed release is a button held forever.
//
//   W.gpadInput.attach(box, ctx)   ctx: {ctl, layout(), haptics(), editing(), changed()}
//   W.gpadInput.releaseAll(box)    everything held goes up (closing, switching off, editing)

(() => {
  const I = (W.gpadInput = {});
  const BUZZ = { off: 0, light: 6, medium: 10, strong: 18 };
  const vibrate = (ctx) => {
    const ms = BUZZ[ctx.haptics ? ctx.haptics() : "medium"] || 0;
    if (ms && navigator.vibrate) { try { navigator.vibrate(ms); } catch (_) {} }
  };

  // −1..1 with the dead zone cut out and the rest stretched back to the full range.
  I.deadZone = (x, y, dz) => {
    const m = Math.hypot(x, y);
    if (m <= dz || m === 0) return [0, 0, m];
    const k = Math.min(1, (m - dz) / (1 - dz)) / m;
    return [x * k, y * k, m];
  };
  // 4-way: the larger axis wins. 8-way: the corners are two directions at once.
  I.dpadDirs = (dx, dy, eight) => {
    if (Math.hypot(dx, dy) <= 0.18) return [];
    if (eight) {
      const s = Math.round(Math.atan2(dy, dx) / (Math.PI / 4));
      return ({ 0: ["dr"], 1: ["dd", "dr"], 2: ["dd"], 3: ["dd", "dl"], 4: ["dl"], "-4": ["dl"], "-3": ["du", "dl"], "-2": ["du"], "-1": ["du", "dr"] })[s] || [];
    }
    return [Math.abs(dx) > Math.abs(dy) ? (dx > 0 ? "dr" : "dl") : (dy > 0 ? "dd" : "du")];
  };
  const WORDS = { du: "up", dd: "down", dl: "left", dr: "right" };
  const valueText = (x, y, mag) => {
    if (mag <= 0) return "centred";
    const v = y < -0.38 ? "up" : y > 0.38 ? "down" : "";
    const h = x < -0.38 ? "left" : x > 0.38 ? "right" : "";
    return `${[v, h].filter(Boolean).join("-") || "centred"}, ${Math.round(Math.min(1, mag) * 100)}%`;
  };

  I.attach = (box, ctx) => {
    if (box._gpIn) { box._gpIn.ctx = ctx; return; }
    const st = (box._gpIn = { ctx, by: new Map() });

    const start = (e) => {
      const ctx = st.ctx;
      if (ctx.editing && ctx.editing()) return;
      const el = e.target.closest && e.target.closest(".gpc");
      if (!el || !el._c || !box.contains(el)) return;
      e.preventDefault();
      try { el.setPointerCapture(e.pointerId); } catch (_) {}
      const c = el._c;
      const p = { el, c, id: e.pointerId };
      st.by.set(e.pointerId, p);
      if (c.kind === "stick" || c.kind === "slider") return stickStart(p, e);
      if (c.kind === "dpad") return dpad(p, e);
      if (c.kind === "pedal") { pedal(p, e); el.classList.add("on"); vibrate(ctx); return; }
      ctx.ctl.button(c.bind, true);
      el.classList.add("on");
      W.gpadMotion.press(el);
      vibrate(ctx);
    };
    const move = (e) => {
      const p = st.by.get(e.pointerId);
      if (!p) return;
      if (p.c.kind === "stick" || p.c.kind === "slider") return stickMove(p, e);
      if (p.c.kind === "dpad") return dpad(p, e);
      if (p.c.kind === "pedal") return pedal(p, e);
    };
    const end = (e) => {
      const p = st.by.get(e.pointerId);
      if (!p) return;
      st.by.delete(e.pointerId);
      up(p);
    };
    function up(p) {
      const { el, c } = p, ctl = st.ctx.ctl;
      el.classList.remove("on");
      if (c.kind === "stick" || c.kind === "slider") return stickEnd(p);
      if (c.kind === "dpad") {
        for (const d of p.dirs || []) ctl.button(d, false);
        for (const a of el.querySelectorAll(".gparm")) a.classList.remove("on");
        return;
      }
      if (c.kind === "pedal") {
        ctl.trigger(c.bind, 0);
        const f = el.querySelector(".gpfill");
        if (f) f.style.scale = "1 0";
        el.setAttribute("aria-valuetext", "released");
        return;
      }
      ctl.button(c.bind, false);
    }

    function dpad(p, e) {
      const r = p.el.getBoundingClientRect();
      const dx = (e.clientX - r.left - r.width / 2) / (r.width / 2), dy = (e.clientY - r.top - r.height / 2) / (r.height / 2);
      const next = I.dpadDirs(dx, dy, p.c.dir === 8);
      const prev = p.dirs || [];
      const ctl = st.ctx.ctl;
      for (const d of prev) if (!next.includes(d)) ctl.button(d, false);
      for (const d of next) if (!prev.includes(d)) ctl.button(d, true);
      if (next.join() !== prev.join() && next.length) vibrate(st.ctx);
      p.dirs = next;
      for (const a of p.el.querySelectorAll(".gparm")) a.classList.toggle("on", next.includes("d" + a.dataset.d));
      p.el.classList.toggle("on", next.length > 0);
    }

    function pedal(p, e) {
      const r = p.el.getBoundingClientRect();
      // Slide up = more. The bottom 15% is "resting on the pedal", already a little gas.
      const v = Math.max(0.15, Math.min(1, ((r.bottom - e.clientY) / r.height) * 1.25));
      st.ctx.ctl.trigger(p.c.bind, v);
      const f = p.el.querySelector(".gpfill");
      if (f) f.style.scale = `1 ${v.toFixed(3)}`;
      p.el.setAttribute("aria-valuetext", `${Math.round(v * 100)}%`);
    }

    function stickStart(p, e) {
      const { el, c } = p, r = el.getBoundingClientRect();
      p.cx = r.left + r.width / 2; p.cy = r.top + r.height / 2; p.R = r.width / 2;
      p.hist = [];
      const L = st.ctx.layout();
      const type = c.kind === "stick" ? L.stick || "fixed" : "fixed";
      if (type !== "fixed") {
        // Floating: the stick comes to the thumb. Relative: zero is wherever the thumb landed.
        p.cx = e.clientX; p.cy = e.clientY;
        if (type === "floating") {
          p.moved = [e.clientX - (r.left + r.width / 2), e.clientY - (r.top + r.height / 2)];
          el.style.translate = `${p.moved[0]}px ${p.moved[1]}px`;
        }
      }
      if (c.kind === "slider" && c.steer === "tilt") tiltOn(p);
      el.classList.add("on");
      vibrate(st.ctx);
      stickMove(p, e);
    }
    function stickMove(p, e) {
      const { el, c } = p;
      if (p.tilt) return;
      const horiz = c.kind === "slider";
      let dx = (e.clientX - p.cx) / (horiz ? p.R * 0.8 : p.R), dy = horiz ? 0 : (e.clientY - p.cy) / p.R;
      const len = Math.hypot(dx, dy);
      // Clamp to the circle, not the square: a diagonal must not read as 1.41×.
      if (len > 1) { dx /= len; dy /= len; }
      stickSet(p, dx, dy);
    }
    function stickSet(p, dx, dy) {
      const { el, c } = p, horiz = c.kind === "slider";
      const L = st.ctx.layout();
      const [x, y, mag] = I.deadZone(dx, dy, L.dead == null ? 0.12 : L.dead);
      const live = x !== 0 || y !== 0;
      if (c.bind === "ls" || c.bind === "rs") st.ctx.ctl.stick(c.bind, x, horiz ? 0 : y);
      el.setAttribute("aria-valuetext", valueText(x, y, live ? mag : 0));
      // The look, after the send: 1:1 under the finger, no easing (direct manipulation).
      const th = el.querySelector(".gpthumb");
      const R = p.R || el.getBoundingClientRect().width / 2;
      if (th) { th.style.translate = `${dx * R * (horiz ? 0.8 : 0.55)}px ${dy * R * 0.55}px`; th.style.opacity = live ? 1 : 0.7; }
      if (!horiz && W.gpadMotion.level() === "full") {
        p.hist.unshift([dx, dy]);
        p.hist.length = Math.min(p.hist.length, 13);
        el.querySelectorAll(".gptrail").forEach((t, i) => {
          const q = p.hist[(i + 1) * 4];
          if (!q) return;
          t.style.translate = `${q[0] * R * 0.55}px ${q[1] * R * 0.55}px`;
          t.style.opacity = live ? ((0.32 - i * 0.09) * Math.min(1, mag)).toFixed(3) : 0;
        });
      }
      const nub = el.querySelector(".gpdir");
      if (nub) { nub.style.rotate = `${(Math.atan2(dy, dx) * 180) / Math.PI + 90}deg`; nub.style.opacity = live ? Math.min(1, mag) : 0; }
      if (st.ctx.changed) st.ctx.changed();
    }
    function stickEnd(p) {
      const { el, c } = p;
      tiltOff(p);
      if (c.bind === "ls" || c.bind === "rs") st.ctx.ctl.stick(c.bind, 0, 0);
      el.setAttribute("aria-valuetext", "centred");
      const th = el.querySelector(".gpthumb");
      if (th) {
        const from = th.style.translate;
        th.style.translate = "";
        th.style.opacity = "";
        // Release: the thumb springs home on --pop (the one place a stick may overshoot).
        W.gpadMotion.home(th, from);
      }
      el.querySelectorAll(".gptrail").forEach((t) => { t.style.opacity = 0; });
      const nub = el.querySelector(".gpdir");
      if (nub) nub.style.opacity = 0;
      if (p.moved) {
        el.style.translate = "";
        W.gpadMotion.floatHome(el, `${p.moved[0]}px ${p.moved[1]}px`);
      }
    }

    // Tilting the phone steers: its roll, ±35° for full lock, while the slider is held.
    function tiltOn(p) {
      if (typeof DeviceOrientationEvent === "undefined") return;
      const go = () => {
        p.tilt = (e) => {
          const landscape = innerWidth > innerHeight;
          const deg = landscape ? (e.beta || 0) * (screen.orientation && screen.orientation.angle === 270 ? -1 : 1) : e.gamma || 0;
          stickSet(p, Math.max(-1, Math.min(1, deg / 35)), 0);
        };
        addEventListener("deviceorientation", p.tilt);
      };
      if (typeof DeviceOrientationEvent.requestPermission === "function") {
        DeviceOrientationEvent.requestPermission().then((r) => { if (r === "granted" && st.by.get(p.id) === p) go(); }).catch(() => {});
      } else go();
    }
    function tiltOff(p) {
      if (p.tilt) removeEventListener("deviceorientation", p.tilt);
      p.tilt = null;
    }

    box.addEventListener("pointerdown", start);
    box.addEventListener("pointermove", move);
    box.addEventListener("pointerup", end);
    box.addEventListener("pointercancel", end);
    box.addEventListener("lostpointercapture", end);
    st.releaseAll = () => {
      for (const p of [...st.by.values()]) { st.by.delete(p.id); up(p); }
    };
  };

  I.releaseAll = (box) => {
    if (box && box._gpIn) box._gpIn.releaseAll();
  };
})();
