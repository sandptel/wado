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
// Push an inflated playout buffer back down once the network has recovered.
//
// `jitterBufferTarget` is a target, not a cap. Chrome grows the buffer in one step when the
// link misbehaves — a single 200 ms RTT spike took it from 8 ms to 68 ms — and then drains it
// at a fraction of a millisecond per second, so one blip costs minutes of added latency that
// the user feels as sluggishness. The hint is set once at connect and never re-evaluated.
//
// The condition is deliberately two-sided: the buffer must be well past what was asked for
// AND the round trip must already be healthy again. Forcing the buffer down while the link is
// still bad is how you trade latency for stutter, and Chrome grew it for a reason. This only
// reclaims the buffer the network no longer needs.
//
// ponytail: re-asserting the same value and letting Chrome converge, rather than tracking a
// target of our own. If Chrome ever stops honouring a repeat set, the next step is stepping
// the target down gradually.
const REASSERT_OVER_TARGET_MS = 25; // how far past the target counts as inflated
const REASSERT_HEALTHY_RTT_MS = 60; // "the link is fine now"
const REASSERT_MIN_GAP_MS = 4000;   // never thrash it
let lastReassert = 0;

W.reassertPlayout = (jbuf, ping) => {
  if (jbuf === null || ping === null || !W.pc) return;
  const target = W.playoutMs || 0;
  if (jbuf <= target + REASSERT_OVER_TARGET_MS) return;
  if (ping > REASSERT_HEALTHY_RTT_MS) return;
  const now = Date.now();
  if (now - lastReassert < REASSERT_MIN_GAP_MS) return;
  lastReassert = now;
  try {
    const recv = W.pc.getReceivers().find((r) => r.track && r.track.kind === "video");
    if (!recv) return;
    W.minimizePlayoutDelay(recv);
    W.rlog && W.rlog(
      "playout reasserted: jbuf=" + Math.round(jbuf) + "ms rtt=" + Math.round(ping) +
      "ms target=" + target + "ms"
    );
  } catch (_) {}
};

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
