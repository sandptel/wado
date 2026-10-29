// wado bridge — this device's trackpad settings. They live typed in the computer's config.kdl,
// under `device "<this device's key>" { trackpad { … } }`, so they follow the device across
// browsers and are editable in the file and `wado tui`. A localStorage mirror lets the pad open
// laid out right before the daemon has answered.
//
// Read: the daemon's copy when `config_state` carries one (`state.trackpad`), else the mirror,
// else the defaults. Write: the mirror at once, then a ConfigSet of
// `device.<W.clientKey>.trackpad.<field>` — booleans as true/false, `quick` as a JSON list
// (`[]` is none), and each gesture its own key `…trackpad.gestures.<name>` (empty turns it off).
//
//   W.padPrefs.get()                      → {landscape, fullscreen, flip, rail, quick, gestures, keyboard}
//   W.padPrefs.set(field, value)          one field; `gestures` goes through setGesture
//   W.padPrefs.setGesture(name, id | "")
//   W.padPrefs.onChange(fn)               fn(prefs), after any change, local or the daemon's
//   W.padPrefs.onReject(fn)               fn(field, message): the daemon refused a save
//   W.padPrefs.RAILS, W.padPrefs.GESTURES

(() => {
  const P = (W.padPrefs = {});
  const MIRROR = "wado.trackpad";
  P.RAILS = ["keys", "actions", "none"];
  P.GESTURES = ["edge-left", "edge-right", "four-tap", "two-hold"];
  const DEFAULTS = {
    landscape: true,
    fullscreen: false,
    flip: false,
    rail: "keys",
    quick: ["workspace-prev", "workspace-next", "fullscreen", "play-pause"],
    gestures: {},
    keyboard: false,
  };
  const changeFns = [];
  const rejectFns = [];

  // Anything from storage or the wire is checked field by field: a bad value falls back to the
  // default rather than breaking the layout.
  function clean(o) {
    const src = o && typeof o === "object" ? o : {};
    const out = { ...DEFAULTS, quick: [...DEFAULTS.quick], gestures: {} };
    for (const k of ["landscape", "fullscreen", "flip", "keyboard"]) {
      if (typeof src[k] === "boolean") out[k] = src[k];
    }
    if (P.RAILS.includes(src.rail)) out.rail = src.rail;
    if (Array.isArray(src.quick)) out.quick = src.quick.map((s) => String(s).trim()).filter(Boolean);
    if (src.gestures && typeof src.gestures === "object") {
      for (const g of P.GESTURES) {
        const id = src.gestures[g];
        if (typeof id === "string" && id) out.gestures[g] = id;
      }
    }
    return out;
  }

  const readMirror = () => { try { return JSON.parse(localStorage.getItem(MIRROR)); } catch (_) { return null; } };
  const writeMirror = () => { try { localStorage.setItem(MIRROR, JSON.stringify(cur)); } catch (_) {} };
  let cur = clean(readMirror());

  P.get = () => ({ ...cur, quick: [...cur.quick], gestures: { ...cur.gestures } });
  P.onChange = (fn) => changeFns.push(fn);
  P.onReject = (fn) => rejectFns.push(fn);
  const emit = () => { const p = P.get(); for (const fn of changeFns) { try { fn(p); } catch (_) {} } };

  const own = (field) => `device.${W.clientKey}.trackpad.${field}`;
  function save(field, wire) {
    writeMirror();
    emit();
    // Private mode has no device key: the setting lives in this browser only.
    if (W.clientKey && W.configSet) W.configSet(own(field), wire);
  }

  P.set = (field, value) => {
    if (!(field in DEFAULTS) || field === "gestures") return;
    const next = clean({ ...cur, [field]: value });
    if (JSON.stringify(next[field]) === JSON.stringify(cur[field])) return;
    cur = next;
    save(field, field === "quick" ? JSON.stringify(cur.quick) : String(cur[field]));
  };

  P.setGesture = (name, id) => {
    if (!P.GESTURES.includes(name)) return;
    const gestures = { ...cur.gestures };
    if (id) gestures[name] = String(id); else delete gestures[name];
    if (JSON.stringify(gestures) === JSON.stringify(cur.gestures)) return;
    cur = { ...cur, gestures };
    save("gestures." + name, id ? String(id) : "");
  };

  // The daemon's copy wins whenever it sends one: that is what lets another browser on the same
  // device key open with the same layout.
  function adopt(t) {
    if (!t || typeof t !== "object") return;
    const next = clean(t);
    if (JSON.stringify(next) === JSON.stringify(cur)) return;
    cur = next;
    writeMirror();
    emit();
  }
  adopt(W.hostConfig && W.hostConfig.trackpad);
  if (W.relayOn) {
    const prevState = W._relayHandlers.config_state;
    W.relayOn("config_state", (m) => { if (prevState) prevState(m); adopt(m && m.state && m.state.trackpad); });
    const prevRej = W._relayHandlers.config_rejected;
    W.relayOn("config_rejected", (m) => {
      if (prevRej) prevRej(m);
      const pre = `device.${W.clientKey}.trackpad.`;
      if (!m || typeof m.key !== "string" || !m.key.startsWith(pre)) return;
      for (const fn of rejectFns) { try { fn(m.key.slice(pre.length), m.message || ""); } catch (_) {} }
    });
  }
})();
