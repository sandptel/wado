// wado bridge — Relay mode **sessions**. Everything that is about a compositor session,
// spoken over the link that `relay_link.js` keeps open.
//
// The split is the point. This file used to own the socket as well, welded into one attempt's
// promise — see the header of `relay_link.js` for what that cost. Here there are no attempts
// and no connection promise: messages arrive, handlers run, and the link is somebody else's
// problem. A session that was streaming when the network went away is resumed by a handler
// firing on a socket this file never opened.
//
// Auth model: a single Remote ID is both the address and the access token. Connecting to
// ws://<relay>/join/<remoteId> IS the join — the first message is join_accepted or join_denied.
//
// Exposed:
//   W.relayConnect(relayUrl, remoteId, config)  ensure the link, then ask for a session
//   W.relayStop()                               stop the session; the link stays up
//   W.relayRejoin() / W.relayDropStart()        the two answers to the session_alive prompt

W._relayAnswer = null;
W._relayConfig = null;

// ── Surviving a page reload ───────────────────────────────────────────────────
//
// `sessionOn` lives in a JS variable, so it does not survive the page being torn down — and on
// a phone the page is torn down constantly: an app switch, a screen lock, the browser evicting
// a background tab. Measured live 2026-09-13 02:14:35, a `pagehide persisted=false` followed by
// a fresh load:
//
//     20:44:35.688  browser: pagehide persisted=false relayMode=true
//     20:44:36.128  browser: relay link up
//     20:44:39.233  a session is already running — offering rejoin or drop
//     20:44:43.558  viewer rejoined the running session      <- 4.2 s waiting for a finger
//
// The session survived perfectly. The *client* forgot it was watching one, so it fell through
// to the prompt and a human had to press a button to get back something they never left.
//
// A crumb in localStorage is what closes it: this device was watching a session moments ago, so
// on the next load it takes it straight back. Deliberately time-limited to the server's grace —
// past that there is nothing to rejoin, and a stale crumb would make every later page load ask
// about a session that has been gone for hours.
const RESUME_KEY = "wado.watching";
const RESUME_TTL_MS = 600000;          // matches VIEWER_GRACE in relay_client.rs

function markWatching(on) {
  try {
    if (on) localStorage.setItem(RESUME_KEY, JSON.stringify({ t: Date.now(), scale: W.outputScale || 1 }));
    else localStorage.removeItem(RESUME_KEY);
  } catch (_) {}
}
function recentlyWatching() {
  try {
    const v = JSON.parse(localStorage.getItem(RESUME_KEY) || "null");
    if (!v || !v.t || Date.now() - v.t > RESUME_TTL_MS) return null;
    return v;
  } catch (_) { return null; }
}

// Called by the link when it comes up on a page that has no session state of its own.
//
// It sends `session_rejoin`, not `session_start`, and that choice is the whole safety argument:
// rejoin can only ever attach to something that already exists. A `session_start` used as the
// query would *create* a session on a daemon that had none — a compositor and an encoder spun
// up because a page loaded, which nobody asked for.
W.relayResumeIfWatching = () => {
  const mark = recentlyWatching();
  if (!mark || W.sessionOn || W._relayWanted) return false;
  // The scroll conversion needs the session's scale and the config is not here on a cold load.
  W.outputScale = mark.scale > 0 ? mark.scale : 1;
  W._relayResuming = true;
  rlog("this device was watching a session — trying to take it back");
  status("relay: reconnecting to your session…");
  return W.relaySendMsg({ type: "session_rejoin" });
};
W._relayWanted = false;     // the viewer has asked for a session and not asked to stop
W._relayChoice = null;      // {config} — no promise; see relayRejoin
W._relayDropPending = false;
W._relayResuming = false;
W._relaySessionTimer = null;

// A phone's console is unreachable mid-field-test, so diagnostics go two ways: into the
// in-page log panel, and over the relay WS to the server, which logs them next to its own.
// That is the only way the two halves of a failed negotiation end up in one place.
// Stage 0 = nothing, 1 = relay reachable, 2 = daemon online, 3 = session up, 4 = video.
// `error` non-empty marks the stage it is passed with as the one that failed.
const phase = (stage, error) => emit({ type: "phase", stage, error: error || "" });
W.relayPhase = phase;
W._relayStage = 0;

const rlog = W.rlog = (line) => {
  emit({ type: "log", line: "INFO|" + new Date().toTimeString().slice(0, 8) + "|browser: " + line });
  W.relaySendMsg({ type: "client_log", line });
};

// ── The one timeout, and what it is actually for ─────────────────────────────
//
// Per *request*, not per connection. The old code had a single 15 s timer covering the dial,
// the join, the session start and the whole WebRTC negotiation, and rejected the lot with one
// of three guessed messages. Dialling and joining now belong to the link, which retries them
// forever instead of giving up; what is left here is the only wait a human is actually blocked
// on — "I pressed Start, is a session coming?" — and it is armed only while that is true.
const SESSION_WAIT_MS = 20000;
function armSessionWait() {
  clearSessionWait();
  W._relaySessionTimer = setTimeout(() => {
    W._relaySessionTimer = null;
    phase(2, "the daemon is online but the session never started");
    status("relay: the daemon did not answer the session request");
    emit({ type: "startFailed" });
  }, SESSION_WAIT_MS);
}
function clearSessionWait() {
  if (W._relaySessionTimer) { clearTimeout(W._relaySessionTimer); W._relaySessionTimer = null; }
}

// ── Main entry point ──────────────────────────────────────────────────────────

W.relayConnect = (relayUrl, remoteId, config) => {
  W._relayConfig = config;
  W._relayWanted = true;
  W.relayMode = true;
  W._relayStage = 0;
  phase(0, "");
  status("relay: connecting to " + relayUrl + "…");
  W.relayDial(relayUrl, remoteId);
  // Warm link — the case this whole split exists for. One message down a socket that is already
  // open: no dial, no TLS handshake through the tunnel, no join, no 15 s timer.
  if (W.relayUp) onLinkUp();
  // Nothing to await. The UI is driven by the emits in the handlers below, so a caller that
  // used to block on a connection promise now returns and lets the stage bar speak.
  return Promise.resolve();
};

// `session_start` doubles as the query: the daemon answers `session_alive` when one is already
// running and `session_started` when it made a new one. A separate SessionQuery message would
// add a protocol variant, a relay pass-through and a server arm to learn what this already says.
function askForSession() {
  if (!W._relayConfig) return;
  armSessionWait();
  W.relaySendMsg({ type: "session_start", config: W._relayConfig });
}

// ── Link events ───────────────────────────────────────────────────────────────

// The link came up — first time, or back after an outage. Both take the same action, and that
// is deliberate: "ask the daemon for a session" already means "…or tell me about the one that
// is running", so there is no branch to get wrong.
function onLinkUp() {
  if (!W._relayWanted || !W._relayConfig) {
    // No session asked for on this page — but this device may have been watching one before the
    // page was torn down under it. See `relayResumeIfWatching`.
    W.relayResumeIfWatching();
    return;
  }
  W.requestApps();
  if (W.sessionOn) {
    // We were streaming when the link went away. Do not prompt — the viewer never chose to
    // leave, and a dialog after a tunnel is a dialog nobody wanted. `session_alive` will be
    // taken straight back; `session_error` means the grace ran out and we start fresh.
    W._relayResuming = true;
    rlog("link back — checking whether the session survived");
    status("relay: reconnecting to the running session…");
  } else {
    status("relay: joined — starting session…");
  }
  askForSession();
}
W.relayOn("__up", onLinkUp);

W.relayOn("__down", () => {
  // A two-message handshake must not survive the socket it was half-spoken on. If the link
  // reconnects between the stop and the start, this flag would fire a session_start nobody
  // asked for.
  W._relayDropPending = false;
  W._relayResuming = false;
  clearSessionWait();
  if (W.sessionOn) status("relay: link lost — holding the session, reconnecting…");
});

// ── Session control responses ─────────────────────────────────────────────────

W.relayOn("session_started", async (msg) => {
  clearSessionWait();
  W._relayResuming = false;
  if (msg.info && msg.info.encoder) {
    emit({ type: "encoder", mode: msg.info.encoder.mode, pipeline: msg.info.encoder.pipeline || "" });
    // The yardsticks the health verdict measures against. They come from the server because
    // only the server knows what `Balanced` resolved to at this resolution — and a decode time
    // judged against a guessed budget accuses the wrong machine.
    W.setTargetKbps(msg.info.encoder.bitrate_kbps || 0);
    W.setTargetFps(msg.info.encoder.fps || 0);
  }
  W.sessionOn = true;
  markWatching(true);
  W._relayStage = 3; phase(3, "");
  rlog("session ready — encoder " + ((msg.info && msg.info.encoder && msg.info.encoder.mode) || "?"));
  stagebar("Session running — negotiating WebRTC…");
  try {
    await W._relayNegotiate();
  } catch (e) {
    rlog("negotiation failed: " + (e && e.message ? e.message : e));
    status("relay: " + (e && e.message ? e.message : e));
  }
});

W.relayOn("session_alive", (msg) => {
  clearSessionWait();
  W._relayStage = 2; phase(2, "");
  if (W._relayResuming) {
    W._relayResuming = false;
    rlog("the session survived the outage — rejoining");
    status("relay: rejoining…");
    armSessionWait();
    W.relaySendMsg({ type: "session_rejoin" });
    return;
  }
  W._relayChoice = { config: W._relayConfig };
  rlog("a session is already running — waiting for rejoin or drop");
  status("relay: a session is already running");
  emit({
    type: "sessionAlive",
    mode: (msg.info && msg.info.encoder && msg.info.encoder.mode) || "",
    pipeline: (msg.info && msg.info.encoder && msg.info.encoder.pipeline) || "",
  });
});

// The running session changed shape. No renegotiation: the track is the same one and the
// decoder picks the new size up from the forced IDR, so the only thing to do here is re-aim the
// health verdict — its decode budget is 1000/fps and its arrival comparison is against the CBR
// target, and both just moved.
W.relayOn("session_reconfigured", (msg) => {
  clearSessionWait();
  if (msg.info && msg.info.encoder) {
    emit({ type: "encoder", mode: msg.info.encoder.mode, pipeline: msg.info.encoder.pipeline || "" });
    W.setTargetKbps(msg.info.encoder.bitrate_kbps || 0);
    W.setTargetFps(msg.info.encoder.fps || 0);
  }
  W.setShedding(1);
  rlog("session reconfigured — " + ((msg.info && msg.info.encoder && msg.info.encoder.bitrate_kbps) || "?") + " kbps");
  status("applied");
  stagebar("Streaming (relay).");
});

W.relayOn("session_error", (msg) => {
  clearSessionWait();
  const why = msg.message || "unknown";
  // The resume case needs its own branch: `sessionOn` deliberately stays true across an outage
  // now, so a client whose session *did* expire would otherwise renegotiate forever against
  // something that is gone. Clear the flag and ask for a fresh one.
  if (W._relayResuming) {
    W._relayResuming = false;
    W.sessionOn = false;
    markWatching(false);
    if (!W._relayWanted || !W._relayConfig) {
      // A cold page load whose crumb turned out to be stale. There is no config here to start
      // from and nobody has pressed anything, so the honest thing is to go quiet and wait.
      rlog("nothing left to rejoin: " + why);
      status("relay: ready — press Start");
      return;
    }
    rlog("the session did not survive: " + why + " — starting a fresh one");
    status("relay: previous session gone — starting fresh");
    askForSession();
    return;
  }
  phase(2, why);
  status("relay: session error — " + why);
  emit({ type: "startFailed" });
});

W.relayOn("session_stopped", () => {
  // Half of a drop-and-restart: the new session cannot be asked for until the old one is
  // actually gone, so the request waits here rather than racing the stop.
  if (W._relayDropPending) {
    W._relayDropPending = false;
    rlog("previous session dropped — starting a new one");
    status("relay: starting session…");
    askForSession();
    return;
  }
  if (W.sessionOn) {
    W.sessionOn = false;
    markWatching(false);
    stagebar("Session stopped.");
  }
});

// ── Server → client state ─────────────────────────────────────────────────────

// The focused application asked for (or gave up) text input. See osk.js — this is what raises
// the phone keyboard on a text field without anyone pressing ⌨.
W.relayOn("text_input", (msg) => W.textInput(!!msg.active));

// The compositor is sending 1 render tick in N. The verdict has to know, or it measures the
// effect of a mitigation this phone asked for and reports it as the server failing.
W.relayOn("shedding", (msg) => W.setShedding(msg.divisor));

W.relayOn("sdp_answer", (msg) => {
  if (W._relayAnswer) { W._relayAnswer(msg.sdp); W._relayAnswer = null; }
});

W.relayOn("ice_candidate", async (msg) => {
  if (W.pc && msg.candidate) {
    try { await W.pc.addIceCandidate(JSON.parse(msg.candidate)); } catch (_) {}
  }
});

// Straight to the emulator rather than through a Dioxus signal: terminal output arrives in
// small bursts at high rate, and routing it through a re-render would make the shell feel
// slower than the video behind it.
W.relayOn("pty_output", (msg) => W.ptyOutput(msg.data || ""));
W.relayOn("pty_exit", () => W.ptyExited());

// Stashed rather than resolved through a promise: the collector runs on its own 1 Hz tick and
// uses the most recent reply, so one dropped answer costs a stale sample instead of a stalled
// breakdown.
W.relayOn("timing", (msg) => { W._lastTiming = msg.timings || null; });

W.relayOn("apps_list", (msg) => emit({ type: "apps", apps: msg.apps || [] }));
W.relayOn("log", (msg) => { if (msg.line) emit({ type: "log", line: msg.line }); });
W.relayOn("error", (msg) => status("relay error: " + (msg.message || "?")));

// ── WebRTC negotiation over the relay link (replaces the HTTP /offer call) ────

W._relayNegotiate = async () => {
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
    // Without this the browser picks its own adaptive jitter buffer, which relay mode was
    // silently living with: measured 23-25 ms of pure queueing on the receiver, on a link
    // with 7-13 ms RTT and no packet loss. Direct mode has always set it here; this is the
    // third thing relay mode was missing that the direct path had (after the ICE servers
    // and the latency echo), so the two ontrack handlers are worth diffing when either moves.
    rlog("playout delay: " + W.minimizePlayoutDelay(
      ev.receiver || pc.getReceivers().find((r) => r.track && r.track.kind === "video")
    ));
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
  if (!W.relaySendMsg({ type: "sdp_offer", sdp: JSON.stringify(pc.localDescription) })) {
    throw new Error("the relay link went away before the offer could be sent");
  }

  // Per-request, and it rejects rather than hanging: the link can reconnect underneath this
  // wait, and a promise nobody ever settles is how a viewer ends up staring at "negotiating".
  const answerSdp = await new Promise((resolve, reject) => {
    W._relayAnswer = resolve;
    setTimeout(() => {
      if (W._relayAnswer !== resolve) return;   // already answered
      W._relayAnswer = null;
      reject(new Error("the daemon did not answer the offer within 30 s"));
    }, 30000);
  });

  rlog("answer received — " + (answerSdp.match(/a=candidate:/g) || []).length + " candidates");
  await pc.setRemoteDescription(JSON.parse(answerSdp));
  rlog("remote description set — ICE checking starts now");
  status("connected via relay");
};

// ── Session control helpers ───────────────────────────────────────────────────

// Every pty verb is fire-and-forget: a keystroke that misses the socket is a keystroke the
// shell never saw, and the terminal showing nothing is the right feedback for that.
const relaySend = (obj) => W.relaySendMsg(obj);

// The client's own verdict on its decoder, going back to the daemon so the render loop can shed
// rather than the viewer having to read a suggestion and change a setting. See js/health.js for
// the hysteresis and the arrival gate; `crates/compositor/src/congestion.rs` for what it does.
W.relayStrain = (strained) => relaySend({ type: "viewer_strain", strained });

// Apply settings to the session that is already running. The applications, the windows and the
// peer connection all survive; see `RelayMsg::SessionReconfigure`.
W.relayReconfigure = (config) => {
  if (!W.sessionOn) return false;
  W._relayConfig = config;
  // The scroll conversion is in logical pixels and the scale may have just changed under it.
  W.outputScale = config && config.scale > 0 ? config.scale : 1;
  armSessionWait();
  return relaySend({ type: "session_reconfigure", config });
};

W.ptyOpen = (cols, rows) => relaySend({ type: "pty_open", cols, rows });
W.ptyInput = (data) => relaySend({ type: "pty_input", data });
W.ptyResize = (cols, rows) => relaySend({ type: "pty_resize", cols, rows });
W.ptyClose = () => relaySend({ type: "pty_close" });

// Stops the *session*. The link stays up, because the page is still open and the next Start
// should be one message rather than a fresh dial — which is the whole point of the split.
W.relayStop = () => {
  clearSessionWait();
  markWatching(false);
  W._relayWanted = false;
  W._relayResuming = false;
  W._relayDropPending = false;
  relaySend({ type: "session_stop" });
};

// ── The answer to `session_alive` ────────────────────────────────────────────
//
// Two exits from one prompt, both driving the link that is already open. Neither re-dials, and
// neither settles a promise: the prompt can now be raised by a reconnect that no `await` is
// waiting on, so anything holding a `resolve` here would throw on the press.

/// Attach to the running session. Windows, applications and their state all survive; the daemon
/// forces a keyframe so the picture starts immediately rather than at the next periodic one.
W.relayRejoin = () => {
  if (!W._relayChoice) return false;
  W._relayChoice = null;
  emit({ type: "sessionAliveCleared" });
  status("relay: rejoining the running session…");
  armSessionWait();
  return relaySend({ type: "session_rejoin" });
};

/// Stop the running session and start a fresh one with *this* viewer's settings.
///
/// Two steps, not one: `session_start` on a live session is what produced the prompt in the
/// first place, so the stop has to be acknowledged before the start is sent. The
/// `session_stopped` handler above is the other half.
W.relayDropStart = () => {
  if (!W._relayChoice) return false;
  W._relayChoice = null;
  W._relayDropPending = true;
  emit({ type: "sessionAliveCleared" });
  status("relay: stopping the previous session…");
  return relaySend({ type: "session_stop" });
};
