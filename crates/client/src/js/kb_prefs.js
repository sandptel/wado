// wado bridge — the keys above this device's keyboard (spec §5.3): the ones a phone keyboard does
// not have, in the order they were added. Typed in the computer's config.kdl under
// `device "<this device's key>" { keyboard { keys … } }`, set by the device itself, with a
// localStorage mirror (`pad_prefs.js` is the pattern). The touchpad screen's key row and the
// console's Keys face draw from it.
//
//   W.kbPrefs.get() → {keys: [...]}     W.kbPrefs.toggle(name)     W.kbPrefs.onChange(fn)
//   W.kbPrefs.NAMES   every key the computer knows by name; "f1" stands for the whole F-row
//   W.kbPrefs.label(name) → what the key says ("←", "PgUp", "F5")

(() => {
  const K = (W.kbPrefs = {});
  const MIRROR = "wado.keyboard";
  const FROW = Array.from({ length: 12 }, (_, i) => `f${i + 1}`);
  K.NAMES = ["esc", "tab", "left", "up", "down", "right", "home", "end", "pageup", "pagedown", "delete", "enter", ...FROW];
  const DEFAULT = ["esc", "tab", "left", "up", "down", "right"];
  const LABEL = { esc: "Esc", tab: "Tab", left: "←", up: "↑", down: "↓", right: "→", home: "Home", end: "End", pageup: "PgUp", pagedown: "PgDn", delete: "Del", enter: "Enter" };
  K.label = (k) => LABEL[k] || k.toUpperCase();
  const fns = [];

  const clean = (list) => {
    if (!Array.isArray(list)) return [...DEFAULT];
    const out = [];
    for (const k of list) if (K.NAMES.includes(k) && !out.includes(k)) out.push(k);
    return out;
  };
  const read = () => { try { const o = JSON.parse(localStorage.getItem(MIRROR)); return o ? clean(o.keys) : [...DEFAULT]; } catch (_) { return [...DEFAULT]; } };
  let keys = read();

  K.get = () => ({ keys: [...keys] });
  K.onChange = (fn) => fns.push(fn);
  const tell = () => {
    for (const fn of fns) { try { fn(K.get()); } catch (_) {} }
    if (typeof emit === "function") emit({ type: "kbPrefs", keys: [...keys] });
  };

  // "f1" toggles the whole function row: twelve keys are one choice.
  K.toggle = (name) => {
    const group = name === "f1" ? FROW : [name];
    const on = keys.includes(group[0]);
    keys = on ? keys.filter((k) => !group.includes(k)) : clean([...keys, ...group]);
    try { localStorage.setItem(MIRROR, JSON.stringify({ keys })); } catch (_) {}
    tell();
    if (W.clientKey && W.configSet) W.configSet(`device.${W.clientKey}.keyboard.keys`, JSON.stringify(keys));
  };

  function adopt(k) {
    if (!k || !Array.isArray(k.keys)) return;
    const next = clean(k.keys);
    if (next.join() === keys.join()) return;
    keys = next;
    try { localStorage.setItem(MIRROR, JSON.stringify({ keys })); } catch (_) {}
    tell();
  }
  adopt(W.hostConfig && W.hostConfig.keyboard);
  if (W.relayOn) {
    const prev = W._relayHandlers.config_state;
    W.relayOn("config_state", (m) => { if (prev) prev(m); adopt(m && m.state && m.state.keyboard); });
  }
  setTimeout(tell, 0);
})();
