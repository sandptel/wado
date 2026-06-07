// wado bridge — session lifecycle (start / launch / stop) + page-lifetime keep-alive.
// MUST be concatenated last: the trailing never-resolving await keeps this eval (and its
// dioxus.send channel) alive for the app's lifetime.

// Ask the server to start a session, then connect the video. `config` is the SessionConfig
// as a JS object (valid JSON the server deserializes).
W.start = async (server, config) => {
  W.server = server;
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
    // The server reports which encoder it actually opened (hardware vs software);
    // surface it so the UI can show the software-encoding banner (invariant #5).
    try {
      const info = await res.json();
      if (info && info.encoder && info.encoder.mode) {
        emit({ type: "encoder", mode: info.encoder.mode });
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
};

// Launch a command into the running session in realtime (callable repeatedly).
W.launch = async (command) => {
  if (!W.sessionOn) { status("launch ignored — no session"); return; }
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
  W.resetInput();
  W.inputDC = null;
  if (W.pc) { try { W.pc.close(); } catch (_) {} W.pc = null; }
  const v = document.getElementById("wado-video");
  if (v) v.srcObject = null;
  stagebar("No session.");
  try { await fetch(W.server + "/session/stop", { method: "POST" }); } catch (_) {}
};

// Free server resources promptly if the tab is closed mid-session.
window.addEventListener("pagehide", () => {
  if (W.sessionOn && W.server) navigator.sendBeacon(W.server + "/session/stop");
});

// Keep this eval (and its `dioxus` send channel) alive for the app's lifetime.
await new Promise(() => {});
