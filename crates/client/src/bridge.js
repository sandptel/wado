// wado client ↔ browser bridge.
//
// This script runs once (kept alive forever by the trailing never-resolving await)
// inside a long-lived Dioxus `eval`. It owns the browser-only pieces — the control
// POSTs (fetch), the WebRTC peer connection, the <video> element's `srcObject`, the
// SSE log stream, and the pagehide beacon — because a MediaStream can only be
// attached from JS and the WebRTC handshake is inherently DOM-bound.
//
// It exposes `window.__wado.{start,stopSession,connectLogs}` for the Rust side to
// call (via tiny one-shot evals) and reports state back to Rust through `dioxus.send`,
// captured here as `emit`. Messages are `{type, ...}` objects:
//   {type:"status",      text} – short status line
//   {type:"stagebar",    text} – the bar above the video
//   {type:"log",         line} – one SSE log line (LEVEL|HH:MM:SS|text)
//   {type:"startFailed"}       – /session/start rejected or threw
//   {type:"giveup"}            – WebRTC failed past the retry budget; tear the session down

window.__wado = window.__wado || {};
const W = window.__wado;
W.pc = null;
W.logES = null;
W.server = "";
W.sessionOn = false;
W.reconnectAttempts = 0;
W.MAX_RECONNECTS = 3;

const emit = (msg) => { try { dioxus.send(msg); } catch (_) {} };
const status = (text) => emit({ type: "status", text });
const stagebar = (text) => emit({ type: "stagebar", text });

// Live logs: SSE → emit each line to Rust, which renders them in the panel.
W.connectLogs = (server) => {
  W.server = server;
  if (W.logES) { try { W.logES.close(); } catch (_) {} }
  const es = new EventSource(server + "/events");
  W.logES = es;
  es.onmessage = (ev) => emit({ type: "log", line: ev.data });
  es.onerror = () => {}; // EventSource auto-reconnects
};

// Build a fresh peer connection + offer. The server makes a new pc per offer, so a
// reconnection is a full re-offer (not an ICE restart on the same pc).
W.connectWebRTC = async () => {
  const server = W.server;
  if (W.pc) { try { W.pc.close(); } catch (_) {} }
  const pc = new RTCPeerConnection();
  W.pc = pc;
  pc.addTransceiver("video", { direction: "recvonly" });
  pc.ontrack = (ev) => {
    const v = document.getElementById("wado-video");
    if (v) v.srcObject = ev.streams[0];
    stagebar("Streaming.");
    W.reconnectAttempts = 0;
  };
  pc.oniceconnectionstatechange = () => status("ICE: " + pc.iceConnectionState);
  pc.onconnectionstatechange = () => {
    // Only `failed` is terminal; `disconnected` is transient and usually recovers.
    if (W.pc && W.pc.connectionState === "failed") W.handleFailure();
  };

  const offer = await pc.createOffer();
  await pc.setLocalDescription(offer);
  await new Promise((resolve) => {
    if (pc.iceGatheringState === "complete") return resolve();
    const check = () => {
      if (pc.iceGatheringState === "complete") {
        pc.removeEventListener("icegatheringstatechange", check);
        resolve();
      }
    };
    pc.addEventListener("icegatheringstatechange", check);
  });

  const resp = await fetch(server + "/offer", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(pc.localDescription),
  });
  if (!resp.ok) throw new Error("offer rejected: HTTP " + resp.status);
  await pc.setRemoteDescription(await resp.json());
  status("connected");
};

// Retry the WebRTC connection with backoff; the compositor session keeps running.
W.handleFailure = () => {
  if (!W.sessionOn) return;
  if (W.reconnectAttempts >= W.MAX_RECONNECTS) {
    status("connection lost — giving up");
    emit({ type: "giveup" });
    return;
  }
  W.reconnectAttempts++;
  const delay = 500 * Math.pow(2, W.reconnectAttempts - 1);
  status(`connection lost — reconnecting (${W.reconnectAttempts}/${W.MAX_RECONNECTS})…`);
  setTimeout(() => {
    if (!W.sessionOn) return;
    W.connectWebRTC().catch(() => W.handleFailure());
  }, delay);
};

// Ask the server to start a session, then connect the video. `config` is the
// SessionConfig as a JS object (valid JSON the server deserializes).
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
    W.sessionOn = true;
    W.reconnectAttempts = 0;
    stagebar("Session running — connecting video…");
    await W.connectWebRTC();
  } catch (e) {
    status("error: " + (e && e.message ? e.message : e));
    emit({ type: "startFailed" });
  }
};

W.stopSession = async () => {
  W.sessionOn = false;
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
