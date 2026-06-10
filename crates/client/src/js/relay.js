// wado bridge — Relay mode. Manages the WebSocket connection to wado-relay,
// acting as the full signaling + session control channel when relay mode is
// selected. Replaces the direct HTTP path (/session/start, /offer, /events).
//
// Auth model: a single Remote ID is both the address and the access token.
// Connecting to ws://<relay>/join/<remoteId> IS the join — no join message;
// the first message from the relay is join_accepted or join_denied.
//
// Exposed:
//   W.relayConnect(relayUrl, remoteId, config)
//     Opens relay WS, sends SessionStart on acceptance, negotiates WebRTC,
//     resolves when the peer connection is established.
//
//   W.relayStop()
//     Sends SessionStop via relay and closes the WS.
//
//   W.relayLaunch(command)
//     Sends SessionLaunch via relay.
//
// State:
//   W.relayWs         — the active relay WebSocket (null when idle)
//   W.relayMode       — true while a relay connection is active
//   W._relayAnswer    — resolves with the SDP answer JSON string

W.relayWs = null;
W.relayMode = false;
W._relayAnswer = null;

// ── Main entry point ──────────────────────────────────────────────────────────

W.relayConnect = async (relayUrl, remoteId, config) => {
  W.relayMode = true;
  if (W.relayWs) { try { W.relayWs.close(); } catch (_) {} W.relayWs = null; }

  // Normalize the Remote ID: 528-491-307 / "528 491 307" / 528491307 are equal.
  const id = String(remoteId).replace(/[\s-]/g, "");

  const wsUrl = relayUrl.replace(/^https?:\/\//, (m) => m === "https://" ? "wss://" : "ws://")
                         .replace(/^ws(s?):\/\/(.*)$/, (_, s, rest) => `ws${s}://${rest}`)
               + "/join/" + encodeURIComponent(id);

  status("relay: connecting to " + relayUrl + "…");

  return new Promise((resolve, reject) => {
    const ws = new WebSocket(wsUrl);
    W.relayWs = ws;

    const timeout = setTimeout(() => {
      reject(new Error("relay: connection timed out"));
      ws.close();
    }, 15000);

    ws.onerror = () => {
      clearTimeout(timeout);
      reject(new Error("relay: WebSocket error"));
    };

    ws.onmessage = async (ev) => {
      let msg;
      try { msg = JSON.parse(ev.data); } catch (_) { return; }

      switch (msg.type) {
        // ── Handshake (the WS path was the join; just await the verdict) ─────
        case "join_accepted":
          status("relay: joined — starting session…");
          // Ask the server to start a compositor session.
          ws.send(JSON.stringify({ type: "session_start", config }));
          break;

        case "join_denied":
          clearTimeout(timeout);
          reject(new Error("relay: " + (msg.reason || "join denied")));
          ws.close();
          break;

        // ── Session control responses ────────────────────────────────────────
        case "session_started":
          // Surface encoder info (invariant #5 — software banner).
          if (msg.info && msg.info.encoder) {
            emit({ type: "encoder", mode: msg.info.encoder.mode, pipeline: msg.info.encoder.pipeline || "" });
          }
          W.sessionOn = true;
          stagebar("Session running — negotiating WebRTC…");
          // Now negotiate WebRTC through the relay.
          try {
            await W._relayNegotiate(ws);
            clearTimeout(timeout);
            resolve();
          } catch (e) {
            clearTimeout(timeout);
            reject(e);
          }
          break;

        case "session_error":
          clearTimeout(timeout);
          status("relay: session error — " + (msg.message || "unknown"));
          emit({ type: "startFailed" });
          reject(new Error("relay: session error: " + (msg.message || "unknown")));
          ws.close();
          break;

        case "session_stopped":
          if (W.sessionOn) {
            W.sessionOn = false;
            stagebar("Session stopped.");
          }
          break;

        // ── WebRTC signaling ─────────────────────────────────────────────────
        case "sdp_answer":
          if (W._relayAnswer) { W._relayAnswer(msg.sdp); W._relayAnswer = null; }
          break;

        case "ice_candidate":
          if (W.pc && msg.candidate) {
            try { await W.pc.addIceCandidate(JSON.parse(msg.candidate)); } catch (_) {}
          }
          break;

        // ── Live logs forwarded from the server ───────────────────────────────
        case "log":
          if (msg.line) emit({ type: "log", line: msg.line });
          break;

        // ── Keepalive ────────────────────────────────────────────────────────
        case "ping":
          ws.send(JSON.stringify({ type: "pong" }));
          break;

        case "error":
          status("relay error: " + (msg.message || "?"));
          break;

        default:
          break;
      }
    };

    ws.onclose = () => {
      W.relayWs = null;
      if (W.sessionOn) status("relay: connection closed");
    };
  });
};

// ── WebRTC negotiation via relay WS (replaces the HTTP /offer call) ───────────

W._relayNegotiate = async (ws) => {
  if (W.pc) { try { W.pc.close(); } catch (_) {} }

  const pc = new RTCPeerConnection({
    iceServers: [
      { urls: "stun:stun.l.google.com:19302" },
      { urls: "stun:stun1.l.google.com:19302" },
    ],
  });
  W.pc = pc;

  pc.addTransceiver("video", { direction: "recvonly" });
  W.inputDC = pc.createDataChannel(INPUT_CHANNEL, { ordered: true });

  pc.ontrack = (ev) => {
    const v = document.getElementById("wado-video");
    if (v) v.srcObject = ev.streams[0];
    stagebar("Streaming (relay).");
    W.reconnectAttempts = 0;
    W.startStats(pc);
    W.setupInputCapture();
  };

  pc.oniceconnectionstatechange = () => status("ICE: " + pc.iceConnectionState);
  pc.onconnectionstatechange = () => {
    if (W.pc && W.pc.connectionState === "failed") W.handleFailure();
  };

  const offer = await pc.createOffer();
  await pc.setLocalDescription(offer);

  // Wait for ICE gathering before sending (non-trickle, consistent with server).
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

  ws.send(JSON.stringify({ type: "sdp_offer", sdp: JSON.stringify(pc.localDescription) }));

  // Wait for SDP answer (relay forwards it from server).
  const answerSdp = await new Promise((resolve, reject) => {
    W._relayAnswer = resolve;
    setTimeout(() => {
      W._relayAnswer = null;
      reject(new Error("relay: SDP answer timed out (30 s)"));
    }, 30000);
  });

  await pc.setRemoteDescription(JSON.parse(answerSdp));
  status("connected via relay");
};

// ── Session control helpers ───────────────────────────────────────────────────

W.relayStop = () => {
  if (W.relayWs && W.relayWs.readyState === WebSocket.OPEN) {
    W.relayWs.send(JSON.stringify({ type: "session_stop" }));
    W.relayWs.close();
  }
  W.relayWs = null;
  W.relayMode = false;
};

W.relayLaunch = async (command) => {
  if (!W.relayWs || W.relayWs.readyState !== WebSocket.OPEN) {
    status("relay: not connected");
    return;
  }
  W.relayWs.send(JSON.stringify({ type: "session_launch", command }));
};
