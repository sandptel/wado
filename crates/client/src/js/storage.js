// wado bridge — settings persistence. One JSON blob under one key, because the settings are
// only ever read and written as a whole; per-key storage would buy nothing and cost a
// migration story. Unknown keys survive a round trip and missing ones fall back to serde
// defaults on the Rust side, so an old blob never breaks a new build.
//
// This is per-browser, not per-user: nothing here reaches the server.

const STORE_KEY = "wado.settings";

W.loadSettings = () => {
  try {
    return JSON.parse(localStorage.getItem(STORE_KEY)) || {};
  } catch (_) {
    return {}; // private mode, disabled storage, or a corrupt blob — defaults are fine
  }
};

// A connect link — `?relay=<url>&id=<remote id>`, which scripts/rig.sh prints — wins over what
// was saved, and is saved in its place: a quick tunnel rotates its URL on every restart, and a
// phone holding the old one fails silently, never reaching the relay at all. Applied here,
// before anything reads the settings, then removed from the address bar so a reload or a
// bookmark does not keep re-applying it.
(() => {
  let q;
  try { q = new URLSearchParams(location.search); } catch (_) { return; }
  const relay = q.get("relay"), id = q.get("id");
  if (!relay && !id) return;
  const s = W.loadSettings();
  s.conn_mode = "relay";
  if (relay) s.relay_url = relay;
  if (id) s.remote_id = id;
  // The selected host card too, or it would still show — and Start would save back — the old one.
  if (Array.isArray(s.profiles) && s.profiles[s.profile || 0]) {
    const p = s.profiles[s.profile || 0];
    p.conn_mode = "relay";
    if (relay) p.relay_url = relay;
    if (id) p.remote_id = id;
  }
  try { localStorage.setItem(STORE_KEY, JSON.stringify(s)); } catch (_) {}
  try { history.replaceState(null, "", location.pathname); } catch (_) {}
})();

W.saveSettings = (obj) => {
  try { localStorage.setItem(STORE_KEY, JSON.stringify(obj)); } catch (_) {}
};
