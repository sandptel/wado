// wado bridge — this device's heartbeat to the computer. One job: tell the daemon how this
// device reaches it (`RelayMsg::DeviceStatus`), so `wado tui` and `wado` can show every
// connected device's path, RTT and network, and the daemon's log records each change of path.
//
// Every 5 s while joined, on each change of path, and once on each link-up. Small and sealed,
// so it rides the relay socket even when there is no peer connection.

function sendDeviceStatus() {
  if (!W.relayUp) return;
  const l = W.linkNow || {};
  const r = W.lastRecovery;
  W.relaySendMsg({
    type: "device_status",
    link: {
      state: l.state || "",
      path: l.path || "",
      rtt_ms: l.rtt_ms == null ? null : l.rtt_ms,
      net: l.net || "",
      visible: document.visibilityState === "visible",
      last_recovery_ms: r ? r.ms : null,
      last_recovery_why: r ? r.why : "",
    },
  });
}

setInterval(sendDeviceStatus, 5000);
// …and at once when the state or path changes, so a short-lived state is not missed between beats.
{
  let seen = "";
  setInterval(() => {
    const l = W.linkNow || {};
    const k = l.state + "|" + l.path + "|" + l.net;
    if (k !== seen) { seen = k; sendDeviceStatus(); }
  }, 1000);
}
{
  const up = W._relayHandlers.__up;
  // After link_state.js's handler has classified the fresh link, so the first report is current.
  W.relayOn("__up", (m) => { if (up) up(m); setTimeout(sendDeviceStatus, 300); });
}
