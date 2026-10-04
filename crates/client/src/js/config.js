// wado bridge — the daemon's config (config.kdl) as this viewer sees it.
//
// Asked for on every link-up, pushed again by the daemon on every reload. Two things ride on
// it: the host's limits and pins, which the control centre greys controls out by, and this
// device's own settings blob, which lives on the daemon (ui.kdl) so it follows the device
// across browsers. After the first answer, every local save is mirrored back — debounced,
// because a slider drag saves on every step.

W.hostConfig = null;
W._prefsSynced = false;
let prefsTimer = 0;

W.relayOn("config_state", (msg) => {
  W.hostConfig = msg.state || null;
  // The newer copy of this device's settings wins. The daemon's used to win always, so a change
  // made just before a reload — inside the 1.5 s debounce, or while the link was down — was
  // reverted by the stale copy on the next connect (measured: "Stream sound" off came back on,
  // 2026-10-04). If ours is newer, drop theirs and send ours.
  let pushOurs = false;
  if (W.hostConfig && W.hostConfig.prefs) {
    let theirAt = 0;
    try { theirAt = JSON.parse(W.hostConfig.prefs)._at || 0; } catch (_) {}
    if (W.settingsAt() > theirAt) { W.hostConfig = { ...W.hostConfig, prefs: null }; pushOurs = true; }
  }
  emit({ type: "hostConfig", state: W.hostConfig });
  W._prefsSynced = true;
  if (pushOurs) {
    try { W.relaySendMsg({ type: "config_set_prefs", prefs: localStorage.getItem("wado.settings") }); } catch (_) {}
  }
});
W.relayOn("config_rejected", (msg) =>
  emit({ type: "configRejected", key: msg.key || "", message: msg.message || "" }));

W.configSet = (key, value, confirmed) =>
  W.relaySendMsg({ type: "config_set", key, value: String(value ?? ""), confirmed: !!confirmed });

// A pending mirror is sent at once when the page is hidden or unloads — a reload inside the
// debounce must not leave the daemon with the old copy.
const flushPrefs = () => {
  if (!prefsTimer || !W._prefsSynced) return;
  clearTimeout(prefsTimer);
  prefsTimer = 0;
  try { W.relaySendMsg({ type: "config_set_prefs", prefs: localStorage.getItem("wado.settings") }); } catch (_) {}
};
addEventListener("pagehide", flushPrefs);
addEventListener("visibilitychange", () => { if (document.visibilityState === "hidden") flushPrefs(); });

{
  const save = W.saveSettings;
  W.saveSettings = (obj) => {
    save(obj);
    if (!W._prefsSynced) return;
    clearTimeout(prefsTimer);
    prefsTimer = setTimeout(() => {
      prefsTimer = 0;
      W.relaySendMsg({ type: "config_set_prefs", prefs: JSON.stringify(obj) });
    }, 1500);
  };
}

{
  const up = W._relayHandlers.__up;
  W.relayOn("__up", (m) => {
    if (up) up(m);
    W._prefsSynced = false;
    W.relaySendMsg({ type: "config_get" });
  });
}
