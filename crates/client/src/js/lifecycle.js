// wado bridge — session lifecycle (start / stop) + page-lifetime keep-alive.
// Launch and the window actions live in control.js; this file only starts and ends sessions.
// MUST be concatenated last: the trailing never-resolving await keeps this eval (and its
// dioxus.send channel) alive for the app's lifetime.
//
// Supports two connection modes:
//   Direct mode  — POST /session/start to server, then HTTP /offer for WebRTC.
//   Relay mode   — all control + WebRTC signaling goes through wado-relay WS.
//                  relayOpts = { relayUrl, remoteId } enables relay mode.

W.start = async (server, config, relayOpts) => {
  W.server = server;
  // The scroll paths convert CSS pixels to the session's logical pixels and need this scale
  // to do it. Taken from the config the UI already hands us rather than plumbed separately.
  W.outputScale = config && config.scale > 0 ? config.scale : 1;
  // Before either branch: a session with no touch input for minutes would otherwise let the
  // phone sleep, and the resulting pagehide tears the session down.
  W.wake.acquire();

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

W.stopSession = async () => {
  W.sessionOn = false;
  W.wake.release();
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
