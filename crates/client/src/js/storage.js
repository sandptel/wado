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
  // The QR's single-use pairing code: kept until the computer lets this device in, proven inside
  // the envelope (never sent), so the host's gate trusts this device without anyone approving it.
  const pair = q.get("pair");
  if (pair) { try { localStorage.setItem("wado.pair", pair); } catch (_) {} }
  // The computer's identity pin, from a QR its owner showed: authoritative, so it replaces any
  // earlier pin for this Remote ID (e2e_keys.js reads the same key).
  const hk = q.get("hk"), hkId = String(q.get("id") || "").replace(/\D/g, "");
  if (hk && hkId) {
    try {
      const all = JSON.parse(localStorage.getItem("wado.hostpins")) || {};
      all[hkId] = { pin: hk, from: "qr" };
      localStorage.setItem("wado.hostpins", JSON.stringify(all));
    } catch (_) {}
  }
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

// Every save carries when it was made (`_at`), so the daemon's copy of these settings and this
// browser's can be told apart by age — see config.js.
W.saveSettings = (obj) => {
  obj._at = Date.now();
  try { localStorage.setItem(STORE_KEY, JSON.stringify(obj)); } catch (_) {}
};
W.settingsAt = () => {
  try { return (JSON.parse(localStorage.getItem(STORE_KEY) || "{}")._at) || 0; } catch (_) { return 0; }
};
