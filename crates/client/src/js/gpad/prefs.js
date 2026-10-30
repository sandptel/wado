// wado bridge — this device's gamepad settings and saved layouts. Typed in the computer's
// config.kdl under `device "<this device's key>" { gamepad { … } }`, set by the device itself
// without the settings grant, as its trackpad's are (`pad_prefs.js` is the pattern). A
// localStorage mirror lets the pad open laid out before the daemon answers (§9.2).
//
// Fields: drives ("pad" | "keys"), layout (the layout in a session and the console deck), away
// ("same", or a layout id for play away from a session), haptics (off | light | medium | strong,
// shared by all three devices), hand ("right" | "left": mirrors the gamepad and the touchpad's
// rail), mine (≤ 8 saved layouts, each {id, name, base, json} — json is the diff, `gpad/model.js`).
//
// Editing a built-in never changes it: the first change forks it into a saved layout of its own
// (`mine-<base>`, named after it), and that becomes the one in use. "Reset" is Undo-able.
//
//   W.gpadPrefs.get() · set(field, v) · onChange(fn) · onReject(fn)
//   W.gpadPrefs.idFor("session" | "away")   the layout id in use there ("same" resolved)
//   W.gpadPrefs.layoutFor(where)            → full layout (mirrored for the left hand)
//   W.gpadPrefs.layoutOf(id)                → full layout of any built-in or saved id
//   W.gpadPrefs.list()                      → built-ins then yours: {id, name, sub, mine, base}
//   W.gpadPrefs.select(where, id) · save(where, full) → id · saveAs(full, name) → id | null
//   W.gpadPrefs.remove(id)

(() => {
  const V = W.gpadVariants, M = W.gpadModel;
  const P = (W.gpadPrefs = {});
  const MIRROR = "wado.gamepad";
  const MAX_MINE = 8;
  P.HAPTICS = ["off", "light", "medium", "strong"];
  const DEFAULTS = { drives: "pad", layout: V.DEFAULT, away: V.SAME, haptics: "medium", hand: "right", mine: [] };
  const changeFns = [], rejectFns = [];

  function clean(o) {
    const src = o && typeof o === "object" ? o : {};
    const out = { ...DEFAULTS, mine: [] };
    if (src.drives === "pad" || src.drives === "keys") out.drives = src.drives;
    if (P.HAPTICS.includes(src.haptics)) out.haptics = src.haptics;
    if (src.hand === "left" || src.hand === "right") out.hand = src.hand;
    if (Array.isArray(src.mine)) {
      for (const m of src.mine) {
        if (!m || typeof m.id !== "string" || !m.id.startsWith("mine-") || out.mine.length >= MAX_MINE) continue;
        out.mine.push({ id: m.id, name: String(m.name || "My layout"), base: V.BUILT[m.base] ? m.base : V.DEFAULT, json: String(m.json || "{}") });
      }
    }
    const known = (id) => !!V.BUILT[id] || out.mine.some((m) => m.id === id);
    if (typeof src.layout === "string" && known(src.layout)) out.layout = src.layout;
    if (src.away === V.SAME || (typeof src.away === "string" && known(src.away))) out.away = src.away;
    return out;
  }
  const readJSON = (k) => { try { return JSON.parse(localStorage.getItem(k)); } catch (_) { return null; } };
  const writeMirror = () => { try { localStorage.setItem(MIRROR, JSON.stringify(cur)); } catch (_) {} };
  let cur = clean(readJSON(MIRROR));
  let pushPending = false;

  P.get = () => JSON.parse(JSON.stringify(cur));
  P.onChange = (fn) => changeFns.push(fn);
  P.onReject = (fn) => rejectFns.push(fn);
  // Every change: the page's listeners, then Rust (`ui/computer/input/prefs.rs`).
  const changed = () => {
    for (const fn of changeFns) { try { fn(P.get()); } catch (_) {} }
    P.tell();
  };
  P.tell = () => {
    const L = P.layoutOf(P.idFor("away"));
    if (typeof emit === "function") emit({ type: "gpadPrefs", prefs: P.get(), list: P.list(), stick: L.stick, dead: L.dead });
  };
  const own = (k) => `device.${W.clientKey}.gamepad.${k}`;
  const wire = (k, v) => { if (W.clientKey && W.configSet) W.configSet(own(k), v); else pushPending = true; };
  const mineWire = (m) => wire("mine." + m.id, m ? JSON.stringify([m.name, m.base, m.json]) : "");

  P.set = (field, value) => {
    if (!(field in DEFAULTS) || field === "mine") return;
    const next = clean({ ...cur, [field]: value });
    if (next[field] === cur[field]) return;
    cur = next;
    writeMirror();
    changed();
    wire(field, cur[field]);
  };

  P.idFor = (where) => (where === "away" && cur.away !== V.SAME ? cur.away : cur.layout);

  P.layoutOf = (id) => {
    const m = cur.mine.find((x) => x.id === id);
    if (!m) return M.builtin(id);
    let diff = {};
    try { diff = JSON.parse(m.json); } catch (_) {}
    return M.resolve(m.base, diff, m.id, m.name);
  };
  const handed = (L) => (cur.hand === "left" ? M.mirror(L) : L);
  P.layoutFor = (where) => handed(P.layoutOf(P.idFor(where)));

  P.list = () => [
    ...V.ORDER.map((id) => ({ id, name: V.BUILT[id].name, sub: V.BUILT[id].sub, mine: false, base: id })),
    ...cur.mine.map((m) => ({ id: m.id, name: m.name, sub: `Yours · from ${V.BUILT[m.base].name}`, mine: true, base: m.base })),
  ];

  P.select = (where, id) => {
    if (where === "away") return P.set("away", id);
    P.set("layout", id);
  };

  function put(m) {
    const i = cur.mine.findIndex((x) => x.id === m.id);
    if (i >= 0) cur.mine[i] = m; else cur.mine.push(m);
    writeMirror();
    changed();
    mineWire(m);
  }

  // Save an edited layout in place; a built-in forks into `mine-<base>` first.
  P.save = (where, full) => {
    const L = handed(full); // back to right-handed coordinates (mirror is its own inverse)
    let id = L.id;
    let name = L.name;
    if (!id.startsWith("mine-")) {
      id = "mine-" + L.base;
      name = V.BUILT[L.base].name;
      if (!cur.mine.some((m) => m.id === id) && cur.mine.length >= MAX_MINE) return null;
    }
    put({ id, name, base: L.base, json: JSON.stringify(M.diff(L)) });
    if (P.idFor(where) !== id) {
      if (where === "away" && cur.away !== V.SAME) P.set("away", id);
      else P.set("layout", id);
    }
    return id;
  };

  P.saveAs = (full, name) => {
    if (cur.mine.length >= MAX_MINE) return null;
    const L = handed(full);
    let id;
    do { id = "mine-" + Math.random().toString(16).slice(2, 6); } while (cur.mine.some((m) => m.id === id));
    put({ id, name: name || `My layout ${cur.mine.length + 1}`, base: L.base, json: JSON.stringify(M.diff(L)) });
    return id;
  };

  P.remove = (id) => {
    const i = cur.mine.findIndex((m) => m.id === id);
    if (i < 0) return;
    cur.mine.splice(i, 1);
    if (cur.layout === id) cur.layout = V.DEFAULT;
    if (cur.away === id) cur.away = V.SAME;
    writeMirror();
    changed();
    wire("mine." + id, "");
    wire("layout", cur.layout);
    wire("away", cur.away);
  };

  // ── migration (§9.2): the old in-session overrides become "My layout (from before)" ────────
  // `wado.padLayout` held {<old id>: {x, y, s}} as fractions of the stage; `pad_mode`,
  // `pad_scale` and `pad_opacity` lived in the settings blob. Done once per browser.
  const OLD = { l1: "lb", l2: "lt", r1: "rb", r2: "rt", select: "back", guide: "guide", start: "start", a: "a", b: "b", x: "x", y: "y", "stick-l": "stick-l", "stick-r": "stick-r" };
  P.migrate = () => {
    try { if (localStorage.getItem("wado.gpadMigrated")) return false; } catch (_) { return false; }
    const old = readJSON("wado.padLayout") || {};
    const blob = readJSON("wado.settings") || {};
    const scale = typeof blob.pad_scale === "number" ? blob.pad_scale : 1;
    const op = typeof blob.pad_opacity === "number" ? blob.pad_opacity : 0.5;
    try { localStorage.setItem("wado.gpadMigrated", "1"); } catch (_) {}
    if (blob.pad_mode === "pad" || blob.pad_mode === "keys") { cur.drives = blob.pad_mode; pushPending = true; }
    const moved = Object.keys(old).length > 0;
    if (!moved && Math.abs(scale - 1) < 0.01 && Math.abs(op - 0.5) < 0.01) { writeMirror(); return false; }
    const L = M.builtin(V.DEFAULT);
    L.look.op = Math.max(0.3, Math.min(1, op * 1.6)); // the old overlay was see-through by default
    const dpad = ["up", "down", "left", "right"].map((k) => old[k]).filter(Boolean);
    for (const c of L.controls) {
      const o = c.id === "dpad" && dpad.length
        ? { x: dpad.reduce((a, d) => a + d.x, 0) / dpad.length, y: dpad.reduce((a, d) => a + d.y, 0) / dpad.length, s: dpad[0].s || 1 }
        : old[Object.keys(OLD).find((k) => OLD[k] === c.id)];
      if (o && typeof o.x === "number" && typeof o.y === "number") { c.l[0] = o.x; c.l[1] = o.y; }
      c.l[2] *= ((o && o.s) || 1) * scale;
      c.p[2] *= scale;
    }
    for (const [id, lab] of [["lt", "L2"], ["lb", "L1"], ["rb", "R1"], ["rt", "R2"]]) L.controls.find((c) => c.id === id).label = lab;
    M.clamp(L);
    const m = { id: "mine-before", name: "My layout (from before)", base: V.DEFAULT, json: JSON.stringify(M.diff(L)) };
    cur.mine = cur.mine.filter((x) => x.id !== m.id).concat([m]).slice(0, MAX_MINE);
    cur.layout = m.id;
    writeMirror();
    pushPending = true;
    try { localStorage.removeItem("wado.padLayout"); } catch (_) {}
    return true;
  };

  // The daemon's copy wins whenever it sends one — unless this browser has something to push
  // that the daemon has never seen (a migration, a change made with no link).
  function adopt(g) {
    if (pushPending && W.clientKey && W.configSet) {
      pushPending = false;
      for (const k of ["drives", "layout", "away", "haptics", "hand"]) W.configSet(own(k), cur[k]);
      for (const m of cur.mine) mineWire(m);
      return;
    }
    if (!g || typeof g !== "object") return;
    const next = clean(g);
    if (JSON.stringify(next) === JSON.stringify(cur)) return;
    cur = next;
    writeMirror();
    changed();
  }
  P.migrate();
  setTimeout(P.tell, 0);
  if (W.hostConfig) adopt(W.hostConfig.gamepad);
  if (W.relayOn) {
    const prev = W._relayHandlers.config_state;
    W.relayOn("config_state", (m) => { if (prev) prev(m); adopt(m && m.state && m.state.gamepad); });
    const prevRej = W._relayHandlers.config_rejected;
    W.relayOn("config_rejected", (m) => {
      if (prevRej) prevRej(m);
      const pre = `device.${W.clientKey}.gamepad.`;
      if (!m || typeof m.key !== "string" || !m.key.startsWith(pre)) return;
      for (const fn of rejectFns) { try { fn(m.key.slice(pre.length), m.message || ""); } catch (_) {} }
    });
  }
})();
