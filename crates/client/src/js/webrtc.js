// wado bridge — WebRTC. Builds a fresh peer connection + offer per connect (the server makes
// a new pc per offer, so a reconnect is a full re-offer). Creates the reliable+ordered input
// data channel before the offer so it lands in the SDP and the server picks it up via
// on_data_channel. On track, attaches the MediaStream, starts stats, and wires input capture.

W.connectWebRTC = async () => {
  const server = W.server;
  W.stopStats();
  if (W.pc) { try { W.pc.close(); } catch (_) {} }
  const pc = new RTCPeerConnection();
  W.pc = pc;
  pc.addTransceiver("video", { direction: "recvonly" });
  // Two input channels (invariant #1). Reliable+ordered for anything unrecoverable if
  // lost (buttons, keys, scroll, touch, drag start/end); ordered + zero-retransmit for
  // high-rate positional updates, which are latest-wins and must never be able to back up
  // behind a retransmit queue. Both are created before the offer so they land in the SDP.
  //
  // Motion is `ordered: true` deliberately. It carries ABSOLUTE positions, so an
  // out-of-order arrival replays a stale position over a newer one and the pointer jumps
  // backwards — jitter, indistinguishable from a network problem. SCTP ordered+unreliable
  // still never retransmits and never head-of-line blocks on a *lost* message; ordering
  // only delays a message behind one that is still in flight, which is exactly the
  // guarantee we want here. Unordered would need a sequence number and a server-side
  // newest-wins filter to be correct, which is strictly more machinery for no latency win.
  W.inputDC = pc.createDataChannel(INPUT_CHANNEL, { ordered: true });
  W.motionDC = pc.createDataChannel(MOTION_CHANNEL, { ordered: true, maxRetransmits: 0 });
  pc.ontrack = (ev) => {
    W.attachStream(ev.streams[0]);
    W.minimizePlayoutDelay(ev.receiver || pc.getReceivers().find((r) => r.track && r.track.kind === "video"));
    stagebar("Streaming.");
    W.reconnectAttempts = 0;
    W.startStats(pc);
    W.attachLatencyEcho();
    if (W.debugLatency) W.latency.start(pc);
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

// Ask the receiver for the SMALLEST playout buffer it will give us.
//
// This is the single biggest source of "lag even at 0 ms ping". A browser's video jitter
// buffer defaults to smooth playback, not low latency, and holds ~100 ms+ of frames — and
// it adapts, so the delay also wanders, which reads as jitter. None of it shows up in
// `currentRoundTripTime`, which is why the stagebar can honestly say 0 ms while the
// picture visibly trails. Interactive remote desktop wants the opposite trade: show the
// newest frame now, accept an occasional hitch.
//
// `jitterBufferTarget` is the standardised knob; `playoutDelayHint` is the older
// Chrome-only spelling. Set whichever exists — both are hints, so the browser still keeps
// a small floor, and neither throws if unsupported.
//
// NOT zero. Zero was the first attempt and it traded one complaint for another: the buffer
// exists to absorb arrival jitter, so removing it entirely makes every few-millisecond
// variation in packet arrival land straight on the screen as judder. That gets worse as
// fps rises — the same 3 ms of wobble is a fifth of a 60 fps frame but nearly half of a
// 120 fps one — which is why the stream looked smoother at 60 than at 120 with the same
// network. A small floor costs a fraction of a frame of latency and buys back smoothness.
// Poke `W.playoutMs` from the console to feel the trade either way.
W.playoutMs = 20;
// ⚠️ Reclaiming an inflated playout buffer by re-asserting the hint DOES NOT WORK, and the
// code that tried was deleted rather than left looking like a feature. Recorded here so it
// is not re-attempted:
//
// v0.0.2 shipped a `reassertPlayout` that re-set `jitterBufferTarget` whenever jbuf ran well
// past it and the RTT was healthy again. It fired — 15 "playout reasserted" lines in one
// session — and the buffer drained 56→55→54→54→53→52 across them, which is its natural rate
// with no inflection at any reassert. It changed nothing.
//
// Why: `jitterBufferTarget` is a *floor*, honoured only up to what the browser's own timing
// model demands. Playout is roughly max(target, model), so the hint can raise the delay and
// can never lower one the model is driving. Two windowed traces confirm it directly —
// `jtarget` tracks `jbuf` (26/24 on desktop, 23/31 on a phone) while the hint reads back as
// the 20 we set. The model binds; our number does not.
//
// So the only thing worth setting is the floor below, once, at connect.

W.minimizePlayoutDelay = (recv) => {
  if (!recv) return "no receiver";
  const applied = [];
  try {
    const ms = Math.max(0, W.playoutMs || 0);
    if ("jitterBufferTarget" in recv) {
      recv.jitterBufferTarget = ms;
      // Read back, not just set: a hint the browser silently declines looks identical to
      // one it honours until you compare this against jbuf.
      applied.push("jitterBufferTarget=" + ms + "(readback=" + recv.jitterBufferTarget + ")");
    }
    if ("playoutDelayHint" in recv) { recv.playoutDelayHint = ms / 1000; applied.push("playoutDelayHint=" + ms + "ms"); }
  } catch (e) {
    return "threw: " + e;
  }
  // Returned rather than logged here: this file is shared with direct mode, which has no
  // relay socket to log down. Both are hints the browser may exceed under real jitter, so
  // knowing they were *set* is not the same as knowing they took — compare against jbuf.
  return applied.length ? applied.join(" ") : "unsupported by this browser";
};

// Re-offer over whichever signalling path this session is using.
//
// The bug this exists to fix: `connectWebRTC` POSTs to `W.server + "/offer"`, an endpoint that
// only exists in direct mode. In relay mode every recovery — `handleFailure` *and* `resync` —
// spent its attempts on a fetch that could never succeed and then printed "giving up". The
// relay-mode re-offer was already written (`W._relayNegotiate`); nothing called it.
//
// The relay link is a separate connection from the peer connection, so it is normally still up
// when ICE dies — which is the whole point: stay joined to the relay, rebuild only WebRTC. And
// when it is *not* up, this is not the code that has to fix it: `relay_link.js` reconnects on
// its own, and its `__up` handler re-asks for the session. Rejecting here just costs one retry.
W.reconnectWebRTC = () => {
  if (!W.relayMode) return W.connectWebRTC();
  if (!W.relayUp) {
    return Promise.reject(new Error("the relay link is down — it is reconnecting on its own"));
  }
  W.stopStats();
  return W._relayNegotiate();
};

// Retry the WebRTC connection with backoff; the compositor session keeps running.
//
// The budget is sized against the server's `VIEWER_GRACE`, which is the only thing that stops a
// session: giving up before it expires strands a session the viewer could still have reclaimed.
// The grace is 600 s now (see `relay_client.rs` — a detached session costs nothing to keep), so
// the budget went 10 → 30 attempts. Capped exponential — 0.5, 1, 2, 4, then 5 s — sums to ~2 min,
// after which the relay link is still up and a reconnect will pick the session back up anyway.
W.handleFailure = () => {
  if (!W.sessionOn) return;
  if (W.reconnectAttempts >= W.MAX_RECONNECTS) {
    // **Relay mode never gives up on the session, only on retrying.**
    //
    // `giveup` calls `stopSession`, which sends `session_stop` — so running out of WebRTC
    // retries used to *destroy* a session the daemon was holding for another eight minutes.
    // That is the exact inverse of what this branch is for, and it fired in precisely the case
    // that motivated the work: a dead zone longer than the retry budget.
    //
    // What replaces it: stop retrying and say so. The relay link is still up and reconnecting
    // on its own, and its `__up` handler asks for the session back — so recovery is the link's
    // job, not this loop's. Direct mode has no such link and no server-side grace, so it keeps
    // the old behaviour.
    if (W.relayMode) {
      status("no media path to the daemon — your session is still running; ⟳ Resync to retry");
      stagebar("Session held — waiting for a network path.");
      return;
    }
    status("connection lost — giving up");
    emit({ type: "giveup" });
    return;
  }
  W.reconnectAttempts++;
  const delay = Math.min(500 * Math.pow(2, W.reconnectAttempts - 1), 5000);
  status(`connection lost — reconnecting (${W.reconnectAttempts}/${W.MAX_RECONNECTS})…`);
  setTimeout(() => {
    if (!W.sessionOn) return;
    W.reconnectWebRTC().catch(() => W.handleFailure());
  }, delay);
};

// Tear down the peer connection and build a fresh one, leaving the compositor session running.
//
// This is the only lever left against the jitter buffer's permanent inflation. A network hitch
// raises the browser's playout target by roughly 5 ms and it never comes back down: the buffer
// is the receiver's, `jitterBufferTarget` is a floor the browser's own timing model outranks
// (see the ⚠️ note above), and nothing in the API resets it. A receiver, however, is created per
// peer connection — so a re-offer resets it by construction.
//
// Deliberately manual. An automatic re-offer keyed on a jbuf threshold would fire hardest on a
// bad link, where dropping the connection is the worst available move; that is the same mistake
// `reassertPlayout` made, one layer up. The compositor session is not tied to the peer
// connection, so windows, applications and the shell all survive this.
W.resync = async () => {
  if (!W.sessionOn) { status("resync: no session"); return; }
  stagebar("Resyncing…");
  W.reconnectAttempts = 0;
  try {
    await W.reconnectWebRTC();
    status("resync: new peer connection");
  } catch (e) {
    status("resync failed: " + e);
    W.handleFailure();
  }
};
