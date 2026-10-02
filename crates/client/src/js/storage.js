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
  // The computer it names: the saved one with that Remote ID, pointed at this relay — or, for a
  // computer this device has never seen, a new card (named by its hostname once it answers).
  const norm = (x) => String(x || "").replace(/\D/g, "");
  s.profiles = Array.isArray(s.profiles) ? s.profiles : [];
  let i = id ? s.profiles.findIndex((p) => norm(p.remote_id) === norm(id)) : (s.profile || 0);
  if (i < 0 || !s.profiles[i]) {
    s.profiles.push({ name: "", host: "", conn_mode: "relay", relay_url: relay || s.relay_url || "", remote_id: id || "" });
    i = s.profiles.length - 1;
  }
  const p = s.profiles[i];
  p.conn_mode = "relay";
  if (relay) p.relay_url = relay;
  if (id) p.remote_id = id;
  s.profile = i;
  // Remember the relay among this device's relays.
  s.relays = Array.isArray(s.relays) ? s.relays : [];
  if (relay && !s.relays.includes(relay)) s.relays.push(relay);
  try { localStorage.setItem(STORE_KEY, JSON.stringify(s)); } catch (_) {}
  try { history.replaceState(null, "", location.pathname); } catch (_) {}
})();

W.saveSettings = (obj) => {
  try { localStorage.setItem(STORE_KEY, JSON.stringify(obj)); } catch (_) {}
};
