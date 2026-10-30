// wado bridge — the gamepad's layout model (spec §9.1): a built-in variant in full, a saved layout
// as a diff on top of one, and the moves between them. Pure: no DOM, so `scripts/gamepad-check.mjs`
// runs it as shipped.
//
// Full form (what the renderer draws):
//   {v:1, id, name, base, look:{shape,style,labels,press,op}, stick, dead, controls:[{id, kind,
//    bind, label, g, l:[x,y,s], p:[x,y,s], dir?, steer?, icon?, word?, look?, hidden?}]}
// Saved form (what is stored, `device.<k>.gamepad.mine.<id>`):
//   {v:1, look?, stick?, dead?, controls:{<id>: {l?, p?, look?, hidden?, dir?, steer?, label?}}}
//   — only what differs from `base`; the id, name and base travel beside it.
//
//   W.gpadModel.builtin(id)            → full layout of a built-in (unknown id: the default)
//   W.gpadModel.resolve(base, diff, id, name) → full layout
//   W.gpadModel.diff(full)             → saved form against full.base
//   W.gpadModel.minScale(kind)         the smallest size that keeps 44 px on the short side
//   W.gpadModel.clamp(full)            every size into [minScale, MAX_SCALE], every position into the area
//   W.gpadModel.mirror(full)           left ↔ right, both orientations (Mirror; Left-handed)
//   W.gpadModel.name(c)                what the control is called ("A button", "Left stick")
//   W.gpadModel.dirty(full)            it differs from its base

(() => {
  const V = W.gpadVariants;
  const M = (W.gpadModel = {});
  const clone = (o) => JSON.parse(JSON.stringify(o));
  const LOOK_KEYS = {
    shape: ["circle", "pill", "round"], style: ["solid", "outline", "flat"],
    labels: ["letters", "shapes", "colors"], press: ["glow", "ring", "still"],
  };
  M.STICKS = ["fixed", "floating", "relative"];
  M.DEAD = 0.12;

  M.minScale = (kind) => {
    const b = V.BASE[kind] || V.BASE.btn;
    return V.MIN_PX / Math.min(b[0], b[1]);
  };

  M.builtin = (id) => {
    const key = V.BUILT[id] ? id : V.DEFAULT;
    const v = V.BUILT[key];
    return M.clamp({ v: 1, id: key, name: v.name, base: key, look: clone(v.look), stick: "fixed", dead: M.DEAD, controls: clone(v.controls) });
  };

  // A look object with only known keys and values; `op` kept within 0.3..1.
  function cleanLook(o) {
    const out = {};
    if (!o || typeof o !== "object") return out;
    for (const k in LOOK_KEYS) if (LOOK_KEYS[k].includes(o[k])) out[k] = o[k];
    if (typeof o.op === "number" && isFinite(o.op)) out.op = Math.min(1, Math.max(0.3, o.op));
    return out;
  }
  const num = (x, lo, hi) => (typeof x === "number" && isFinite(x) ? Math.min(hi, Math.max(lo, x)) : null);
  function cleanPos(a, kind) {
    if (!Array.isArray(a) || a.length < 2) return null;
    const x = num(a[0], 0, 1), y = num(a[1], 0, 1);
    if (x == null || y == null) return null;
    const s = num(a[2] == null ? 1 : a[2], M.minScale(kind), V.MAX_SCALE);
    return [x, y, s == null ? 1 : s];
  }

  M.resolve = (base, diff, id, name) => {
    const L = M.builtin(base);
    const d = diff && typeof diff === "object" ? diff : {};
    L.id = id || L.id;
    L.name = name || L.name;
    L.look = { ...L.look, ...cleanLook(d.look) };
    if (M.STICKS.includes(d.stick)) L.stick = d.stick;
    const dead = num(d.dead, 0, 0.4);
    if (dead != null) L.dead = dead;
    const ov = d.controls && typeof d.controls === "object" ? d.controls : {};
    for (const c of L.controls) {
      const o = ov[c.id];
      if (!o || typeof o !== "object") continue;
      for (const k of ["l", "p"]) { const p = cleanPos(o[k], c.kind); if (p) c[k] = p; }
      const lk = cleanLook(o.look);
      if (Object.keys(lk).length) c.look = lk;
      if (o.hidden === true) c.hidden = true;
      if (c.kind === "dpad" && (o.dir === 4 || o.dir === 8)) c.dir = o.dir;
      if (c.kind === "slider" && (o.steer === "slide" || o.steer === "tilt")) c.steer = o.steer;
      if (typeof o.label === "string" && o.label.length <= 8) c.label = o.label;
    }
    return M.clamp(L);
  };

  const near = (a, b) => Array.isArray(a) && Array.isArray(b) && a.every((x, i) => Math.abs(x - b[i]) < 1e-4);

  M.diff = (full) => {
    const B = M.builtin(full.base);
    const out = { v: 1 };
    const look = {};
    for (const k in full.look) if (full.look[k] !== B.look[k]) look[k] = full.look[k];
    if (Object.keys(look).length) out.look = look;
    if (full.stick && full.stick !== B.stick) out.stick = full.stick;
    if (typeof full.dead === "number" && Math.abs(full.dead - B.dead) > 1e-4) out.dead = +full.dead.toFixed(3);
    const controls = {};
    for (const c of full.controls) {
      const b = B.controls.find((x) => x.id === c.id);
      if (!b) continue;
      const o = {};
      for (const k of ["l", "p"]) if (!near(c[k], b[k])) o[k] = c[k].map((x) => +x.toFixed(4));
      if (c.look && Object.keys(c.look).length) o.look = c.look;
      if (c.hidden) o.hidden = true;
      if (c.dir && c.dir !== (b.dir || 4)) o.dir = c.dir;
      if (c.steer && c.steer !== (b.steer || "slide")) o.steer = c.steer;
      if (c.label !== b.label) o.label = c.label;
      if (Object.keys(o).length) controls[c.id] = o;
    }
    out.controls = controls;
    return out;
  };

  M.dirty = (full) => {
    const d = M.diff(full);
    return !!(d.look || d.stick || d.dead != null || Object.keys(d.controls).length);
  };

  // Keep every control whole on the reference area and at least 44 px on its short side.
  M.clamp = (L) => {
    for (const c of L.controls) {
      for (const o of ["l", "p"]) {
        const pos = c[o];
        pos[2] = Math.min(V.MAX_SCALE, Math.max(M.minScale(c.kind), pos[2] || 1));
        M.clampPos(c, o);
      }
    }
    return L;
  };
  M.clampPos = (c, o) => {
    const [DW, DH] = V.REF[o], pos = c[o];
    const b = V.BASE[c.kind] || V.BASE.btn;
    const hw = Math.min(0.5, (b[0] * pos[2]) / 2 / DW), hh = Math.min(0.5, (b[1] * pos[2]) / 2 / DH);
    pos[0] = Math.min(1 - hw, Math.max(hw, pos[0]));
    pos[1] = Math.min(1 - hh, Math.max(hh, pos[1]));
  };

  M.mirror = (full) => {
    const L = clone(full);
    const flip = (x) => +(1 - x).toFixed(6); // so mirroring twice is the same layout, not a float off
    for (const c of L.controls) { c.l[0] = flip(c.l[0]); c.p[0] = flip(c.p[0]); }
    return L;
  };

  M.lookOf = (c, L) => ({ ...L.look, ...(c.look || {}) });

  M.name = (c) => {
    if (c.kind === "btn") return `${c.label || V.NAMES[c.bind] || c.bind.toUpperCase()} button`;
    if (c.kind === "pedal") return c.label ? `${c.label} pedal` : `${V.NAMES[c.bind]} pedal`;
    if (c.kind === "small") return V.NAMES[c.bind] || c.label || c.id;
    if (c.kind === "bumper" || c.kind === "trig") return c.label || V.NAMES[c.bind];
    return V.NAMES[c.id] || c.label || c.id;
  };

  // The group a control moves with, as ids.
  M.group = (L, id) => {
    const c = L.controls.find((x) => x.id === id);
    return c ? L.controls.filter((x) => x.g === c.g && !x.hidden).map((x) => x.id) : [];
  };
})();
