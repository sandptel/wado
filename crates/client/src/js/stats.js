// wado bridge — live telemetry. Polls RTCPeerConnection.getStats() once a second and emits
// decode FPS + transport round-trip time to Rust. FPS comes from the video inbound-rtp report
// (`framesPerSecond`, or a `framesDecoded` delta fallback); ping is the selected ICE
// candidate-pair's `currentRoundTripTime` (remote inbound-rtp `roundTripTime` is a fallback).

W.startStats = (pc) => {
  W.stopStats();
  let lastFrames = null, lastTs = null, lastBytes = null, lastByteTs = null;
  let lastLost = null, tick = 0, lastDropped = null, lastRecv = null;
  W.statsTimer = setInterval(async () => {
    if (!W.pc || W.pc !== pc) { W.stopStats(); return; } // pc replaced (reconnect)
    let stats;
    try { stats = await pc.getStats(); } catch (_) { return; }
    let fps = null, ping = null, jbuf = null;
    // Loss and receiver-side drops separate the two causes of a full frame pump: frames that
    // never left the server (loss ~0, fps low) from a saturated link (loss climbing, RTT up).
    let lost = null, recv = null, dropped = null, kbps = null;
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
        // Milliseconds of receiver playout buffer: cumulative delay / frames emitted.
        // This is latency RTT cannot see, so it is the number that tells us whether the
        // browser is sitting on frames (see W.minimizePlayoutDelay in webrtc.js).
        if (typeof r.packetsLost === "number") lost = r.packetsLost;
        if (typeof r.framesReceived === "number") recv = r.framesReceived;
        if (typeof r.framesDropped === "number") dropped = r.framesDropped;
        if (typeof r.bytesReceived === "number" && typeof r.timestamp === "number") {
          if (lastBytes !== null && r.timestamp > lastByteTs) {
            kbps = ((r.bytesReceived - lastBytes) * 8) / (r.timestamp - lastByteTs);
          }
          lastBytes = r.bytesReceived;
          lastByteTs = r.timestamp;
        }
        if (typeof r.jitterBufferDelay === "number" &&
            typeof r.jitterBufferEmittedCount === "number" &&
            r.jitterBufferEmittedCount > 0) {
          jbuf = (r.jitterBufferDelay / r.jitterBufferEmittedCount) * 1000;
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
    // Over the window rather than the session: a device that struggled for ten seconds and
    // then settled should stop warning, and a cumulative percentage never would.
    let decodeDropPct = null;
    if (dropped !== null && recv !== null && lastDropped !== null && lastRecv !== null) {
        const dd = dropped - lastDropped, dr = recv - lastRecv;
        if (dr > 0) decodeDropPct = (dd / dr) * 100;
    }
    if (dropped !== null) lastDropped = dropped;
    if (recv !== null) lastRecv = recv;

    // Reclaim a jitter buffer that the network inflated and no longer needs. Cheap, and a
    // no-op unless the buffer has drifted well past the target while the link is healthy.
    if (W.reassertPlayout) W.reassertPlayout(jbuf, ping);

    emit({ type: "stats", fps, ping, jbuf, decodeDropPct });

    // The UI wants 1 Hz; the relay does not — a log line a second per viewer buries the
    // events worth reading. Ship every fifth tick, and immediately on anything anomalous so
    // a fault is never waiting on the next window.
    const lossDelta = (lost !== null && lastLost !== null) ? lost - lastLost : 0;
    if (lost !== null) lastLost = lost;
    const bad = (fps !== null && fps < 45) || lossDelta > 5 ||
                (ping !== null && ping > 250) || (jbuf !== null && jbuf > 250);
    if (++tick % 5 === 0 || bad) {
      const n = (v, d) => (v === null || v === undefined ? "?" : v.toFixed(d));
      W.rlog((bad ? "ANOMALY " : "stats ") +
        "fps=" + n(fps, 1) + " rtt=" + n(ping, 0) + "ms jbuf=" + n(jbuf, 0) + "ms" +
        " kbps=" + n(kbps, 0) + " lost=" + (lost === null ? "?" : lost) +
        " (+" + lossDelta + ") framesDropped=" + (dropped === null ? "?" : dropped) +
        " framesReceived=" + (recv === null ? "?" : recv));
    }
  }, 1000);
};

W.stopStats = () => {
  if (W.statsTimer) { clearInterval(W.statsTimer); W.statsTimer = null; }
};
