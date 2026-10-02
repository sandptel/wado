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
  emit({ type: "hostConfig", state: W.hostConfig });
  W._prefsSynced = true;
});
W.relayOn("config_rejected", (msg) =>
  emit({ type: "configRejected", key: msg.key || "", message: msg.message || "" }));

W.configSet = (key, value, confirmed) =>
  W.relaySendMsg({ type: "config_set", key, value: String(value ?? ""), confirmed: !!confirmed });

{
  const save = W.saveSettings;
  W.saveSettings = (obj) => {
    save(obj);
    if (!W._prefsSynced) return;
    clearTimeout(prefsTimer);
    prefsTimer = setTimeout(
      () => W.relaySendMsg({ type: "config_set_prefs", prefs: JSON.stringify(obj) }), 1500);
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
