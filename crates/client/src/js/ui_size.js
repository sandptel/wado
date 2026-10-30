// wado bridge — this device's size: its text size and its interface size (DESIGN.md, "Size").
//
// Two numbers on <html>, which the stylesheets derive everything from:
//   --text-scale   every font size (all type is in rem; the root is 16px × this)
//   --ui-scale     the interface size: `zoom` on the app's pages and sheets (size.css), so spacing,
//                  radii, control heights and icons — and the text in them — grow together
// `text` is a percentage (85–150 in steps of 5). `follow` hands the text size to the device
// instead: the phone's font size in the Android app (the WebView's own textZoom is pinned at 100
// and the scale handed over, `native_size.js`), the browser's default size in a browser (the root
// is then `100%` rather than 16px). `interface` is compact, cozy (the design's own), comfortable or
// large.
//
// Typed in the computer's config.kdl under `device "<this device's key>" { size { … } }`, set by
// the device itself without the settings grant, as its console's are (`console_prefs.js` is the
// pattern). Read: the daemon's copy when `config_state` carries one, else the localStorage mirror
// (which is also what the first paint uses), else the defaults.
//
//   W.uiSize.get()              → {text, follow, interface, device, native}
//   W.uiSize.set(field, value)  writes the mirror, applies, then ConfigSet
//   W.uiSize.device(scale)      the phone's font scale changed (Android app)

(() => {
  const S = (W.uiSize = {});
  const MIRROR = "wado.size";
  S.INTERFACES = { compact: 0.9, cozy: 1, comfortable: 1.1, large: 1.2 };
  S.TEXT_MIN = 85;
  S.TEXT_MAX = 150;
  S.TEXT_STEP = 5;
  const DEFAULTS = { text: 100, follow: false, interface: "cozy" };

  function clean(o) {
    const src = o && typeof o === "object" ? o : {};
    let text = Math.round(Number(src.text) / S.TEXT_STEP) * S.TEXT_STEP;
    if (!Number.isFinite(text)) text = DEFAULTS.text;
    return {
      text: Math.min(S.TEXT_MAX, Math.max(S.TEXT_MIN, text)),
      follow: src.follow === true,
      interface: Object.hasOwn(S.INTERFACES, src.interface) ? src.interface : DEFAULTS.interface,
    };
  }
  const readMirror = () => { try { return JSON.parse(localStorage.getItem(MIRROR)); } catch (_) { return null; } };
  const writeMirror = () => { try { localStorage.setItem(MIRROR, JSON.stringify(cur)); } catch (_) {} };
  let cur = clean(readMirror() || DEFAULTS);
  // The phone's own font scale, when the Android app said one; null in a browser.
  const info = window.wadoNativeInfo;
  let device = info && Number(info.fontScale) > 0 ? Number(info.fontScale) : null;

  function apply() {
    const root = document.documentElement.style;
    const follow = cur.follow;
    // Following in a browser: the root is the browser's own default size. In the app: 16px times
    // the phone's scale, since the WebView no longer applies it itself.
    root.setProperty("--text-base", follow && device === null ? "100%" : "16px");
    root.setProperty("--text-scale", String(follow ? device || 1 : cur.text / 100));
    root.setProperty("--ui-scale", String(S.INTERFACES[cur.interface]));
    document.documentElement.dataset.ui = cur.interface;
  }

  S.get = () => ({ ...cur, device, native: device !== null });
  const tell = () => emit({ type: "sizePrefs", ...S.get() });

  S.set = (field, value) => {
    if (!(field in DEFAULTS)) return;
    const next = clean({ ...cur, [field]: value });
    if (next[field] === cur[field]) return;
    cur = next;
    writeMirror();
    apply();
    tell();
    // Private mode has no device key: the size lives in this browser only.
    if (W.clientKey && W.configSet) W.configSet(`device.${W.clientKey}.size.${field}`, cur[field]);
  };

  S.device = (scale) => {
    const n = Number(scale);
    if (!(n > 0) || n === device) return;
    device = n;
    apply();
    tell();
  };

  function adopt(c) {
    if (!c || typeof c !== "object") return;
    const next = clean(c);
    if (JSON.stringify(next) === JSON.stringify(cur)) return;
    cur = next;
    writeMirror();
    apply();
    tell();
  }

  apply();
  adopt(W.hostConfig && W.hostConfig.size);
  if (W.relayOn) {
    const prev = W._relayHandlers.config_state;
    W.relayOn("config_state", (m) => { if (prev) prev(m); adopt(m && m.state && m.state.size); });
  }
  tell();
})();
