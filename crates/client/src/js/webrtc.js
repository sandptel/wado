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
    const v = document.getElementById("wado-video");
    if (v) v.srcObject = ev.streams[0];
    W.minimizePlayoutDelay(ev.receiver || pc.getReceivers().find((r) => r.track && r.track.kind === "video"));
    stagebar("Streaming.");
    W.reconnectAttempts = 0;
    W.startStats(pc);
    W.attachLatencyEcho();
    W.latency.start(pc);
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
W.minimizePlayoutDelay = (recv) => {
  if (!recv) return;
  try {
    const ms = Math.max(0, W.playoutMs || 0);
    if ("jitterBufferTarget" in recv) recv.jitterBufferTarget = ms;
    if ("playoutDelayHint" in recv) recv.playoutDelayHint = ms / 1000; // seconds
  } catch (_) {}
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
