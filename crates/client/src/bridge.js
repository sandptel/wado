// wado client ↔ browser bridge.
//
// This script runs once (kept alive forever by the trailing never-resolving await)
// inside a long-lived Dioxus `eval`. It owns the browser-only pieces — the control
// POSTs (fetch), the WebRTC peer connection, the <video> element's `srcObject`, the
// SSE log stream, and the pagehide beacon — because a MediaStream can only be
// attached from JS and the WebRTC handshake is inherently DOM-bound.
//
// It exposes `window.__wado.{start,stopSession,connectLogs,launch}` for the Rust side to
// call (via tiny one-shot evals) and reports state back to Rust through `dioxus.send`,
// captured here as `emit`. Messages are `{type, ...}` objects:
//   {type:"status",      text} – short status line
//   {type:"stagebar",    text} – the bar above the video
//   {type:"log",         line} – one SSE log line (LEVEL|HH:MM:SS|text)
//   {type:"startFailed"}       – /session/start rejected or threw
//   {type:"giveup"}            – WebRTC failed past the retry budget; tear the session down
//   {type:"stats", fps, ping}  – once/sec live decode FPS + transport RTT (ms); either may be null

window.__wado = window.__wado || {};
const W = window.__wado;
W.pc = null;
W.logES = null;
W.server = "";
W.sessionOn = false;
W.reconnectAttempts = 0;
W.MAX_RECONNECTS = 3;
W.statsTimer = null;
W.inputDC = null;            // reliable+ordered data channel carrying InputEvents
W.pressedKeys = new Set();   // evdev codes currently down (for release-on-blur)
W.activePointers = new Set();// pointerIds currently down (so we only send motion when held)
W.inputCaptureReady = false; // DOM listeners attached once

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

// ── Remote input (touch + keyboard only; no mouse SYSTEM, no cursor) ────────────────
//
// We capture **Pointer Events** (`pointerdown/move/up`), which unify mouse + touch + pen,
// and translate every one into a `wl_touch` contact — so a desktop mouse click becomes a
// touch tap (there is no separate mouse path and no cursor). Coords are normalized 0..1
// against the *displayed video content rect* (object-fit:contain letterbox math done here,
// where we know the element geometry) so the server scales 1:1 to the output. Keyboard
// sends Linux evdev keycodes from KeyboardEvent.code (compositor applies the xkb +8). See
// INPUT_CHALLENGES.md.

const INPUT_CHANNEL = "wado-input"; // must match wado_protocol::INPUT_CHANNEL

// KeyboardEvent.code → Linux evdev keycode (input-event-codes.h). Unknown codes are
// logged and skipped (add them here when they show up).
const CODE_TO_EVDEV = {
  KeyA:30,KeyB:48,KeyC:46,KeyD:32,KeyE:18,KeyF:33,KeyG:34,KeyH:35,KeyI:23,KeyJ:36,
  KeyK:37,KeyL:38,KeyM:50,KeyN:49,KeyO:24,KeyP:25,KeyQ:16,KeyR:19,KeyS:31,KeyT:20,
  KeyU:22,KeyV:47,KeyW:17,KeyX:45,KeyY:21,KeyZ:44,
  Digit1:2,Digit2:3,Digit3:4,Digit4:5,Digit5:6,Digit6:7,Digit7:8,Digit8:9,Digit9:10,Digit0:11,
  Enter:28,Escape:1,Backspace:14,Tab:15,Space:57,
  Minus:12,Equal:13,BracketLeft:26,BracketRight:27,Backslash:43,Semicolon:39,Quote:40,
  Backquote:41,Comma:51,Period:52,Slash:53,
  ShiftLeft:42,ShiftRight:54,ControlLeft:29,ControlRight:97,AltLeft:56,AltRight:100,
  MetaLeft:125,MetaRight:126,CapsLock:58,
  ArrowUp:103,ArrowDown:108,ArrowLeft:105,ArrowRight:106,
  Home:102,End:107,PageUp:104,PageDown:109,Insert:110,Delete:111,
  F1:59,F2:60,F3:61,F4:62,F5:63,F6:64,F7:65,F8:66,F9:67,F10:68,F11:87,F12:88,
};

W.sendInput = (obj) => {
  const dc = W.inputDC;
  if (dc && dc.readyState === "open") {
    try { dc.send(JSON.stringify(obj)); } catch (_) {}
  } else {
    console.warn("input dropped, channel not open:", dc && dc.readyState);
  }
};

// Normalize a client point to 0..1 within the video's rendered (letterboxed) content rect.
const normPoint = (clientX, clientY, video) => {
  const r = video.getBoundingClientRect();
  const vw = video.videoWidth, vh = video.videoHeight;
  if (!vw || !vh || !r.width || !r.height) return null;
  const scale = Math.min(r.width / vw, r.height / vh); // object-fit: contain
  const cw = vw * scale, ch = vh * scale;
  const offX = r.left + (r.width - cw) / 2;
  const offY = r.top + (r.height - ch) / 2;
  const clamp = (v) => Math.min(1, Math.max(0, v));
  return { x: clamp((clientX - offX) / cw), y: clamp((clientY - offY) / ch) };
};

// Attach pointer + keyboard listeners to the <video> once. Pointer events cover mouse,
// touch and pen; each becomes a wl_touch contact. Keyboard fires only while the video is
// focused (tabindex) so the rest of the page stays usable; a click/tap focuses it.
W.setupInputCapture = () => {
  if (W.inputCaptureReady) return;
  const video = document.getElementById("wado-video");
  if (!video) return;
  W.inputCaptureReady = true;
  W.videoEl = video;
  video.tabIndex = 0;
  video.style.touchAction = "none"; // stop browser pan/zoom so we get raw pointer events

  const sendPoint = (id, phase, clientX, clientY) => {
    const n = normPoint(clientX, clientY, video);
    if (n) W.sendInput({ t: "touch", id: id >>> 0, phase, x: n.x, y: n.y });
  };
  video.addEventListener("pointerdown", (e) => {
    e.preventDefault();
    video.focus();
    try { video.setPointerCapture(e.pointerId); } catch (_) {}
    W.activePointers.add(e.pointerId);
    sendPoint(e.pointerId, "down", e.clientX, e.clientY);
  });
  video.addEventListener("pointermove", (e) => {
    if (!W.activePointers.has(e.pointerId)) return; // only while a contact is down
    e.preventDefault();
    sendPoint(e.pointerId, "motion", e.clientX, e.clientY);
  });
  const end = (e) => {
    if (!W.activePointers.has(e.pointerId)) return;
    e.preventDefault();
    W.activePointers.delete(e.pointerId);
    try { video.releasePointerCapture(e.pointerId); } catch (_) {}
    sendPoint(e.pointerId, "up", e.clientX, e.clientY);
  };
  video.addEventListener("pointerup", end);
  video.addEventListener("pointercancel", end);

  // Keyboard at window level, but only forwarded while the video is focused.
  const sendKey = (e, pressed) => {
    if (document.activeElement !== video) return;
    const code = CODE_TO_EVDEV[e.code];
    if (code === undefined) { console.warn("unmapped key:", e.code); return; }
    e.preventDefault();
    if (pressed) W.pressedKeys.add(code); else W.pressedKeys.delete(code);
    W.sendInput({ t: "key", code, pressed });
  };
  window.addEventListener("keydown", (e) => sendKey(e, true));
  window.addEventListener("keyup", (e) => sendKey(e, false));
  // Release any held keys if focus is lost, so nothing sticks down server-side.
  video.addEventListener("blur", () => {
    for (const code of W.pressedKeys) W.sendInput({ t: "key", code, pressed: false });
    W.pressedKeys.clear();
  });
};

// Build a fresh peer connection + offer. The server makes a new pc per offer, so a
// reconnection is a full re-offer (not an ICE restart on the same pc).
W.connectWebRTC = async () => {
  const server = W.server;
  W.stopStats();
  if (W.pc) { try { W.pc.close(); } catch (_) {} }
  const pc = new RTCPeerConnection();
  W.pc = pc;
  pc.addTransceiver("video", { direction: "recvonly" });
  // Reliable+ordered input channel, created before the offer so it's in the SDP and the
  // server picks it up via on_data_channel. (Unreliable motion comes with drag/scroll.)
  W.inputDC = pc.createDataChannel(INPUT_CHANNEL, { ordered: true });
  pc.ontrack = (ev) => {
    const v = document.getElementById("wado-video");
    if (v) v.srcObject = ev.streams[0];
    stagebar("Streaming.");
    W.reconnectAttempts = 0;
    W.startStats(pc);
    W.setupInputCapture();
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

// Poll RTCPeerConnection.getStats() once a second and report live decode FPS +
// transport round-trip time to Rust. FPS comes from the video inbound-rtp report
// (`framesPerSecond`, or computed from the `framesDecoded` delta as a fallback);
// ping is the selected ICE candidate-pair's `currentRoundTripTime` (the remote
// inbound-rtp `roundTripTime` is a fallback). Either may be null until populated.
W.startStats = (pc) => {
  W.stopStats();
  let lastFrames = null, lastTs = null;
  W.statsTimer = setInterval(async () => {
    // Bail if this pc is no longer the active one (e.g. after a reconnect).
    if (!W.pc || W.pc !== pc) { W.stopStats(); return; }
    let stats;
    try { stats = await pc.getStats(); } catch (_) { return; }
    let fps = null, ping = null;
    stats.forEach((r) => {
      if (r.type === "inbound-rtp" && (r.kind === "video" || r.mediaType === "video")) {
        if (typeof r.framesPerSecond === "number") {
          fps = r.framesPerSecond;
        } else if (typeof r.framesDecoded === "number" && typeof r.timestamp === "number") {
          if (lastFrames !== null && r.timestamp > lastTs) {
            fps = ((r.framesDecoded - lastFrames) * 1000) / (r.timestamp - lastTs);
          }
          lastFrames = r.framesDecoded;
          lastTs = r.timestamp;
        }
      } else if (r.type === "candidate-pair" && (r.nominated || r.state === "succeeded")) {
        if (typeof r.currentRoundTripTime === "number") ping = r.currentRoundTripTime * 1000;
      }
    });
    if (ping === null) {
      stats.forEach((r) => {
        if (r.type === "remote-inbound-rtp" && typeof r.roundTripTime === "number") {
          ping = r.roundTripTime * 1000;
        }
      });
    }
    emit({ type: "stats", fps, ping });
  }, 1000);
};

W.stopStats = () => {
  if (W.statsTimer) { clearInterval(W.statsTimer); W.statsTimer = null; }
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
  W.inputDC = null;
  W.pressedKeys.clear();
  W.activePointers.clear();
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
