// wado bridge — session lifecycle (start / launch / stop) + page-lifetime keep-alive.
// MUST be concatenated last: the trailing never-resolving await keeps this eval (and its
// dioxus.send channel) alive for the app's lifetime.
//
// Supports two connection modes:
//   Direct mode  — POST /session/start to server, then HTTP /offer for WebRTC.
//   Relay mode   — all control + WebRTC signaling goes through wado-relay WS.
//                  relayOpts = { relayUrl, remoteId } enables relay mode.

W.start = async (server, config, relayOpts) => {
  W.server = server;

  if (relayOpts && relayOpts.relayUrl) {
    // ── Relay mode ────────────────────────────────────────────────────────────
    status("relay: connecting…");
    try {
      W.reconnectAttempts = 0;
      await W.relayConnect(relayOpts.relayUrl, relayOpts.remoteId, config);
    } catch (e) {
      status("relay error: " + (e && e.message ? e.message : String(e)));
      emit({ type: "startFailed" });
    }
  } else {
    // ── Direct mode (existing behaviour) ─────────────────────────────────────
    status("starting session…");
    try {
      const res = await fetch(server + "/session/start", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(config),
      });
      if (!res.ok) {
        status("start failed: " + (await res.text()));
        emit({ type: "startFailed" });
        return;
      }
      try {
        const info = await res.json();
        if (info && info.encoder && info.encoder.mode) {
          emit({ type: "encoder", mode: info.encoder.mode, pipeline: info.encoder.pipeline || "" });
        }
      } catch (_) {}
      W.sessionOn = true;
      W.reconnectAttempts = 0;
      stagebar("Session running — connecting video…");
      await W.connectWebRTC();
    } catch (e) {
      status("error: " + (e && e.message ? e.message : e));
      emit({ type: "startFailed" });
    }
  }
};

// Launch a command into the running session in realtime (callable repeatedly).
W.launch = async (command) => {
  if (!W.sessionOn) { status("launch ignored — no session"); return; }
  if (W.relayMode) {
    await W.relayLaunch(command);
    return;
  }
  try {
    const res = await fetch(W.server + "/session/launch", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(command),
    });
    if (!res.ok) status("launch failed: " + (await res.text()));
  } catch (e) {
    status("launch error: " + (e && e.message ? e.message : e));
  }
};

W.stopSession = async () => {
  W.sessionOn = false;
  W.stopStats();
  W.latency.stop();
  W.resetInput();
  W.inputDC = null;
  W.motionDC = null;
  if (W.pc) { try { W.pc.close(); } catch (_) {} W.pc = null; }
  const v = document.getElementById("wado-video");
  if (v) v.srcObject = null;
  stagebar("No session.");

  if (W.relayMode) {
    W.relayStop();
  } else {
    try { await fetch(W.server + "/session/stop", { method: "POST" }); } catch (_) {}
  }
};

// Free server resources promptly if the tab is closed mid-session.
window.addEventListener("pagehide", () => {
  if (!W.sessionOn) return;
  if (W.relayMode && W.relayWs && W.relayWs.readyState === WebSocket.OPEN) {
    W.relayWs.send(JSON.stringify({ type: "session_stop" }));
  } else if (W.server) {
    navigator.sendBeacon(W.server + "/session/stop");
  }
});

// Keep this eval (and its `dioxus` send channel) alive for the app's lifetime.
await new Promise(() => {});
