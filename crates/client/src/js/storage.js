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

W.saveSettings = (obj) => {
  try { localStorage.setItem(STORE_KEY, JSON.stringify(obj)); } catch (_) {}
};
