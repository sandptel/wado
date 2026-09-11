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
//     Sends SessionLaunch via relay.
//
// State:
//   W.relayWs         — the active relay WebSocket (null when idle)
//   W.relayMode       — true while a relay connection is active
//   W._relayAnswer    — resolves with the SDP answer JSON string

W.relayWs = null;
W.relayMode = false;
W._relayAnswer = null;

// A phone's console is unreachable mid-field-test, so diagnostics go two ways: into the
// in-page log panel, and over the relay WS to the server, which logs them next to its own.
// That is the only way the two halves of a failed negotiation end up in one place.
// Stage 0 = nothing, 1 = relay reachable, 2 = daemon online, 3 = session up, 4 = video.
// `error` non-empty marks the stage it is passed with as the one that failed.
const phase = (stage, error) => emit({ type: "phase", stage, error: error || "" });

const rlog = (line) => {
  emit({ type: "log", line: "INFO|" + new Date().toTimeString().slice(0, 8) + "|browser: " + line });
  const ws = W.relayWs;
  if (ws && ws.readyState === WebSocket.OPEN) {
    try { ws.send(JSON.stringify({ type: "client_log", line })); } catch (_) {}
  }
};

// ── Main entry point ──────────────────────────────────────────────────────────

W.relayConnect = async (relayUrl, remoteId, config) => {
  W.relayMode = true;
  W._relayStage = 0;
  if (W.relayWs) { try { W.relayWs.close(); } catch (_) {} W.relayWs = null; }

  // Normalize the Remote ID: 528-491-307 / "528 491 307" / 528491307 are equal.
  const id = String(remoteId).replace(/[\s-]/g, "");

  const wsUrl = relayUrl.replace(/^https?:\/\//, (m) => m === "https://" ? "wss://" : "ws://")
                         .replace(/^ws(s?):\/\/(.*)$/, (_, s, rest) => `ws${s}://${rest}`)
               + "/join/" + encodeURIComponent(id);

  phase(0, "");
  status("relay: connecting to " + relayUrl + "…");
  emit({ type: "log", line: "INFO|" + new Date().toTimeString().slice(0, 8) + "|browser: dialing " + wsUrl });

  return new Promise((resolve, reject) => {
    const ws = new WebSocket(wsUrl);
    W.relayWs = ws;

    // Named per stage: the same 15 s expiry used to report "connection timed out" whether
      // the relay was down, the daemon absent, or the encoder merely slow to open.
    const timeout = setTimeout(() => {
      const stuck = ["relay unreachable — check the relay URL and that it is running",
                     "relay reachable but no daemon answered for this Remote ID",
                     "daemon is up but the session never started"][W._relayStage || 0]
                     || "handshake stalled";
      phase(W._relayStage || 0, stuck);
      reject(new Error("relay: " + stuck));
      ws.close();
    }, 15000);

    ws.onopen = () => { W._relayStage = 1; phase(1, ""); rlog("relay WS open — awaiting join verdict"); };

    ws.onerror = () => {
      clearTimeout(timeout);
      reject(new Error("relay: WebSocket error (relay unreachable or TLS/mixed-content blocked)"));
    };

    ws.onmessage = async (ev) => {
      let msg;
      try { msg = JSON.parse(ev.data); } catch (_) { return; }

      switch (msg.type) {
        // ── Handshake (the WS path was the join; just await the verdict) ─────
        case "join_accepted":
          W._relayStage = 2; phase(2, "");
          rlog("join accepted — a daemon is registered for this Remote ID");
          status("relay: joined — starting session…");
          // Ask the server to start a compositor session.
          ws.send(JSON.stringify({ type: "session_start", config }));
          // The socket only exists from here, so this is the earliest the app list can be
          // fetched in relay mode. Direct mode asks at page load instead.
          W.requestApps();
          break;

        case "join_denied":
          phase(1, msg.reason || "join denied");
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
          W._relayStage = 3; phase(3, "");
          rlog("session started — encoder " + ((msg.info && msg.info.encoder && msg.info.encoder.mode) || "?"));
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
          phase(2, msg.message || "session failed to start");
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

        // ── Launchable applications ──────────────────────────────────────────
        case "apps_list":
          emit({ type: "apps", apps: msg.apps || [] });
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

    ws.onclose = (ev) => {
      rlog("relay WS closed — code " + ev.code + (ev.reason ? " " + ev.reason : ""));
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
    phase(4, "");
    rlog("track received — media is flowing");
    const v = document.getElementById("wado-video");
    if (v) v.srcObject = ev.streams[0];
    stagebar("Streaming (relay).");
    W.reconnectAttempts = 0;
    W.startStats(pc);
    // Relay mode used to stop here, so it reported no latency breakdown at all — an absent
    // reading that was easy to misread as a good one. Same wiring as the direct path.
    W.attachLatencyEcho();
    if (W.debugLatency) W.latency.start(pc);
    W.setupInputCapture();
  };

  pc.oniceconnectionstatechange = () => {
    rlog("ICE state: " + pc.iceConnectionState);
    status("ICE: " + pc.iceConnectionState);
    if (pc.iceConnectionState === "failed") {
      phase(3, "no network path to the daemon — ICE failed. Both ends are likely behind " +
               "a NAT that STUN cannot traverse; this needs a TURN server.");
    }
  };
  pc.onconnectionstatechange = () => {
    rlog("peer state: " + pc.connectionState);
    if (W.pc && W.pc.connectionState === "failed") W.handleFailure();
  };

  // Candidate types are the diagnosis. "host" only means STUN never answered — on a
  // carrier NAT that is the end of it, and the fix is a TURN server, not this code.
  const seenTypes = new Set();
  pc.onicecandidate = (ev) => {
    if (!ev.candidate) { rlog("ICE gathering complete — types: " + ([...seenTypes].join(",") || "none")); return; }
    const m = /(?: typ )(\w+)/.exec(ev.candidate.candidate);
    if (m && !seenTypes.has(m[1])) { seenTypes.add(m[1]); rlog("first " + m[1] + " candidate: " + ev.candidate.candidate); }
  };
  pc.onicecandidateerror = (ev) => rlog("ICE candidate error " + ev.errorCode + " from " + ev.url + " — " + ev.errorText);

  const offer = await pc.createOffer();
  await pc.setLocalDescription(offer);

  // Wait for ICE gathering before sending (non-trickle, consistent with server) — but only
  // so long. "complete" needs every configured STUN server to answer or time out, and a
  // phone that had both a host and a srflx candidate within 40 ms was still not complete
  // 14 s later, so the handshake timeout fired and the offer was never sent at all.
  // The candidates that matter arrive first; send what we have and let the rest go.
  await new Promise((resolve) => {
    if (pc.iceGatheringState === "complete") return resolve();
    const done = () => {
      clearTimeout(cap);
      pc.removeEventListener("icegatheringstatechange", check);
      resolve();
    };
    const check = () => { if (pc.iceGatheringState === "complete") done(); };
    const cap = setTimeout(() => {
      rlog("ICE gathering still " + pc.iceGatheringState + " after 3 s — sending what we have");
      done();
    }, 3000);
    pc.addEventListener("icegatheringstatechange", check);
  });

  rlog("sending offer — " + (pc.localDescription.sdp.match(/a=candidate:/g) || []).length + " candidates");
  ws.send(JSON.stringify({ type: "sdp_offer", sdp: JSON.stringify(pc.localDescription) }));

  // Wait for SDP answer (relay forwards it from server).
  const answerSdp = await new Promise((resolve, reject) => {
    W._relayAnswer = resolve;
    setTimeout(() => {
      W._relayAnswer = null;
      reject(new Error("relay: SDP answer timed out (30 s)"));
    }, 30000);
  });

  rlog("answer received — " + (answerSdp.match(/a=candidate:/g) || []).length + " candidates");
  await pc.setRemoteDescription(JSON.parse(answerSdp));
  rlog("remote description set — ICE checking starts now");
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

