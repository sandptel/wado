// wado bridge — this device's console preferences: how a session opens (`open`: "console" or
// "full") and which input face the deck shows (`face`: "auto", "pad", "keys", "quick",
// "gamepad"; "auto" is Touchpad on touch, Quick actions with a mouse). Typed in the computer's
// config.kdl under `device "<this device's key>" { console { … } }`, set by the device itself
// without the settings grant, as the trackpad's are (`pad_prefs.js` is the pattern).
//
// Read: the daemon's copy when `config_state` carries one, else the localStorage mirror, else the
// defaults. Write: the mirror at once, then a ConfigSet of `device.<W.clientKey>.console.<field>`.
//
//   W.consolePrefs.get()            → {open, face}
//   W.consolePrefs.set(field, v)    a face change animates the relayout (`stage_morph.prepare`)

(() => {
  const P = (W.consolePrefs = {});
  const MIRROR = "wado.console";
  P.OPENS = ["console", "full"];
  P.FACES = ["auto", "pad", "keys", "quick", "gamepad"];
  const DEFAULTS = { open: "console", face: "auto" };

  function clean(o) {
    const src = o && typeof o === "object" ? o : {};
    return {
      open: P.OPENS.includes(src.open) ? src.open : DEFAULTS.open,
      face: P.FACES.includes(src.face) ? src.face : DEFAULTS.face,
    };
  }
  const readMirror = () => { try { return JSON.parse(localStorage.getItem(MIRROR)); } catch (_) { return null; } };
  const writeMirror = () => { try { localStorage.setItem(MIRROR, JSON.stringify(cur)); } catch (_) {} };
  let cur = clean(readMirror());

  P.get = () => ({ ...cur });
  const tell = () => emit({ type: "consolePrefs", ...cur });

  P.set = (field, value) => {
    if (!(field in DEFAULTS)) return;
    const next = clean({ ...cur, [field]: value });
    if (next[field] === cur[field]) return;
    if (field === "face") {
      if (W.stageMorph) W.stageMorph.prepare();
      // Nothing stays held on the computer when the keys or the pad go away.
      if (W.deckKeys) W.deckKeys.releaseAll();
      if (W.deckPad) W.deckPad.release();
    }
    cur = next;
    writeMirror();
    tell();
    if (field === "face") {
      const names = { pad: "Touchpad", keys: "Keys", quick: "Quick actions", gamepad: "Gamepad" };
      const el = document.getElementById("stage-announce");
      if (el && names[cur.face]) el.textContent = names[cur.face] + " below the picture.";
    }
    // Private mode has no device key: the preference lives in this browser only.
    if (W.clientKey && W.configSet) W.configSet(`device.${W.clientKey}.console.${field}`, cur[field]);
  };

  function adopt(c) {
    if (!c || typeof c !== "object") return;
    const next = clean(c);
    if (JSON.stringify(next) === JSON.stringify(cur)) return;
    cur = next;
    writeMirror();
    tell();
  }
  adopt(W.hostConfig && W.hostConfig.console);
  if (W.relayOn) {
    const prev = W._relayHandlers.config_state;
    W.relayOn("config_state", (m) => { if (prev) prev(m); adopt(m && m.state && m.state.console); });
  }
  tell();
})();
