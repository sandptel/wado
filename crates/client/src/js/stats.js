// wado bridge — live telemetry. Polls RTCPeerConnection.getStats() once a second and emits
// decode FPS + transport round-trip time to Rust. FPS comes from the video inbound-rtp report
// (`framesPerSecond`, or a `framesDecoded` delta fallback); ping is the selected ICE
// candidate-pair's `currentRoundTripTime` (remote inbound-rtp `roundTripTime` is a fallback).

W.startStats = (pc) => {
  W.stopStats();
  let lastFrames = null, lastTs = null;
  W.statsTimer = setInterval(async () => {
    if (!W.pc || W.pc !== pc) { W.stopStats(); return; } // pc replaced (reconnect)
    let stats;
    try { stats = await pc.getStats(); } catch (_) { return; }
    let fps = null, ping = null, jbuf = null;
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
    emit({ type: "stats", fps, ping, jbuf });
  }, 1000);
};

W.stopStats = () => {
  if (W.statsTimer) { clearInterval(W.statsTimer); W.statsTimer = null; }
};
