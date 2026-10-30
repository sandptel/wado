// wado bridge — the deck's Keys face (`ui/surround/deck/keys.rs`): keys for the session.
//
// Modifiers latch as the trackpad's do (`pad_keys.js`): tap → latched for the next key; tap
// again within 300 ms → locked (held down on the computer until tapped, so Super + drag can move
// a window); tap a latched or locked one → off. A key goes out wrapped in whatever is latched,
// and the latches clear. Linux evdev codes, as `input_keyboard.js` sends them. Delegated from
// the document, so Dioxus may re-mount the face freely.
//
//   W.deckKeys.mods          {super, ctrl, alt, shift}: "off" | "latched" | "locked"
//   W.deckKeys.releaseAll()  the face went away (`console_prefs.js`) or the session ended —
//                            so the face always re-mounts with nothing latched

(() => {
  const K = (W.deckKeys = {});
  const MOD = { super: 125, ctrl: 29, alt: 56, shift: 42 };
  const KEY = { esc: 1, tab: 15, left: 105, right: 106, up: 103, down: 108 };
  const DOUBLE_MS = 300;
  K.mods = { super: "off", ctrl: "off", alt: "off", shift: "off" };
  const lastTap = {};

  const send = (code, pressed) => W.coalesce.now({ t: "key", code, pressed });

  function paint() {
    for (const name in K.mods) {
      document.querySelectorAll(`[data-dmod="${name}"]`).forEach((b) => {
        b.classList.toggle("latched", K.mods[name] === "latched");
        b.classList.toggle("locked", K.mods[name] === "locked");
        b.setAttribute("aria-pressed", String(K.mods[name] !== "off"));
      });
    }
  }

  function tapMod(name) {
    const now = performance.now();
    const st = K.mods[name];
    if (st === "latched" && now - (lastTap[name] || 0) < DOUBLE_MS) {
      K.mods[name] = "locked";
      send(MOD[name], true);
    } else if (st === "off") {
      K.mods[name] = "latched";
    } else {
      if (st === "locked") send(MOD[name], false);
      K.mods[name] = "off";
    }
    lastTap[name] = now;
    paint();
  }

  function tapKey(name) {
    const code = KEY[name];
    if (!code) return;
    const latched = Object.keys(K.mods).filter((m) => K.mods[m] === "latched");
    for (const m of latched) send(MOD[m], true);
    send(code, true);
    send(code, false);
    for (const m of latched.reverse()) { send(MOD[m], false); K.mods[m] = "off"; }
    paint();
  }

  K.releaseAll = () => {
    for (const m in K.mods) {
      if (K.mods[m] === "locked") send(MOD[m], false);
      K.mods[m] = "off";
    }
    paint();
  };

  document.addEventListener("click", (e) => {
    const t = e.target && e.target.closest && e.target.closest("[data-dmod], [data-dkey]");
    if (!t) return;
    if (t.dataset.dmod) tapMod(t.dataset.dmod);
    else tapKey(t.dataset.dkey);
  });
})();
