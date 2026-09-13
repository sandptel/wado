// wado bridge — live telemetry. Polls RTCPeerConnection.getStats() once a second and emits
// decode FPS + transport round-trip time to Rust. FPS comes from the video inbound-rtp report
// (`framesPerSecond`, or a `framesDecoded` delta fallback); ping is the selected ICE
// candidate-pair's `currentRoundTripTime` (remote inbound-rtp `roundTripTime` is a fallback).

let targetFps = 0;
W.setTargetFps = (n) => { targetFps = +n || 0; };

W.startStats = (pc) => {
  W.stopStats();
  let lastFrames = null, lastTs = null, lastBytes = null, lastByteTs = null;
  let lastLost = null, tick = 0, lastDropped = null, lastRecv = null, lastPrecv = null;
  let lastDecTime = null, lastDecFrames = null;
  let lastJDelay = null, lastJTarget = null, lastJCount = null;
  W.statsTimer = setInterval(async () => {
    if (!W.pc || W.pc !== pc) { W.stopStats(); return; } // pc replaced (reconnect)
    // Re-assert the stream against the stage. Free when it is already right, and the only
    // thing that recovers a stream attached before Dioxus painted the element, or attached to
    // an element a re-render has since replaced. See js/video.js.
    W.attachStream();
    let stats;
    try { stats = await pc.getStats(); } catch (_) { return; }
    let fps = null, ping = null, jbuf = null;
    // Loss and receiver-side drops separate the two causes of a full frame pump: frames that
    // never left the server (loss ~0, fps low) from a saturated link (loss climbing, RTT up).
    let lost = null, recv = null, dropped = null, kbps = null;
    // jtarget is what the browser's own timing model is aiming for; jbuf is what it
    // delivered. They separate "our hint is being ignored" from "the hint is not the
    // binding constraint". dec is mean decode time per frame — at 120 fps anything at or
    // past 8.3 ms means the decoder has no headroom, so a backlog can never drain.
    let jtarget = null, dec = null;
    // Read for the verdict in health.js, not for display: jitter separates a delayed path
    // from a lossy one, packetsReceived turns packetsLost into a *rate* (a cumulative count
    // says nothing about now), and availableIncomingBitrate is the only number the browser
    // has about how much link there actually is.
    let jitter = null, precv = null, avail = null;
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
        if (typeof r.packetsReceived === "number") precv = r.packetsReceived;
        if (typeof r.jitter === "number") jitter = r.jitter * 1000;
        if (typeof r.framesReceived === "number") recv = r.framesReceived;
        if (typeof r.framesDropped === "number") dropped = r.framesDropped;
        if (typeof r.bytesReceived === "number" && typeof r.timestamp === "number") {
          if (lastBytes !== null && r.timestamp > lastByteTs) {
            kbps = ((r.bytesReceived - lastBytes) * 8) / (r.timestamp - lastByteTs);
          }
          lastBytes = r.bytesReceived;
          lastByteTs = r.timestamp;
        }
        // Windowed, not cumulative. `jitterBufferDelay / jitterBufferEmittedCount` is a
        // session mean, and a session mean cannot show what the buffer is doing *now*: it
        // converges slowly upward toward a value the buffer already reached, so a step
        // change reads as a gentle ramp, and it lags downward after a spike, so a recovery
        // that already happened still reads as "inflated" for a minute. Both of those
        // misreadings were made off this metric before it was windowed.
        if (typeof r.jitterBufferDelay === "number" &&
            typeof r.jitterBufferEmittedCount === "number") {
          if (lastJCount !== null && r.jitterBufferEmittedCount > lastJCount) {
            const dn = r.jitterBufferEmittedCount - lastJCount;
            jbuf = ((r.jitterBufferDelay - lastJDelay) / dn) * 1000;
            if (typeof r.jitterBufferTargetDelay === "number" && lastJTarget !== null) {
              jtarget = ((r.jitterBufferTargetDelay - lastJTarget) / dn) * 1000;
            }
          }
          lastJDelay = r.jitterBufferDelay;
          lastJCount = r.jitterBufferEmittedCount;
          if (typeof r.jitterBufferTargetDelay === "number") {
            lastJTarget = r.jitterBufferTargetDelay;
          }
        }
        // Over the window, not the session — same reason as decodeDropPct below. A
        // cumulative mean stays pinned to the startup burst for minutes and reads as a
        // per-frame cost the decoder is not actually paying now.
        if (typeof r.totalDecodeTime === "number" &&
            typeof r.framesDecoded === "number") {
          if (lastDecTime !== null && r.framesDecoded > lastDecFrames) {
            dec = ((r.totalDecodeTime - lastDecTime) /
                   (r.framesDecoded - lastDecFrames)) * 1000;
          }
          lastDecTime = r.totalDecodeTime;
          lastDecFrames = r.framesDecoded;
        }
      } else if (r.type === "candidate-pair" && (r.nominated || r.state === "succeeded")) {
        if (typeof r.currentRoundTripTime === "number") ping = r.currentRoundTripTime * 1000;
        if (typeof r.availableIncomingBitrate === "number") avail = r.availableIncomingBitrate / 1000;
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

    // Loss as a rate over this window, not the session total: a stream that lost 400 packets
    // in its first minute and none since reads identically to one losing them continuously,
    // and only one of those is a fault you can still do something about.
    let lossPct = null;
    if (lost !== null && precv !== null && lastLost !== null && lastPrecv !== null) {
      const dl = lost - lastLost, dp = precv - lastPrecv;
      if (dl + dp > 0) lossPct = (dl / (dl + dp)) * 100;
    }
    if (precv !== null) lastPrecv = precv;

    emit({ type: "stats", fps, ping, jbuf, decodeDropPct });

    // The verdict runs off the same snapshot rather than polling getStats a second time.
    W.health({ fps, ping, jbuf, dec, jitter, kbps, lossPct, decodeDropPct,
               availableKbps: avail, targetFps });

    // The UI wants 1 Hz; the relay does not — a log line a second per viewer buries the
    // events worth reading. Ship every fifth tick, and immediately on anything anomalous so
    // a fault is never waiting on the next window.
    const lossDelta = (lost !== null && lastLost !== null) ? lost - lastLost : 0;
    const bad = (fps !== null && fps < 45) || lossDelta > 5 ||
                (ping !== null && ping > 250) || (jbuf !== null && jbuf > 250);
    if (++tick % 5 === 0 || bad) {
      const n = (v, d) => (v === null || v === undefined ? "?" : v.toFixed(d));
      W.rlog((bad ? "ANOMALY " : "stats ") +
        "fps=" + n(fps, 1) + " rtt=" + n(ping, 0) + "ms jbuf=" + n(jbuf, 0) + "ms" +
        " kbps=" + n(kbps, 0) + " lost=" + (lost === null ? "?" : lost) +
        " (+" + lossDelta + ") framesDropped=" + (dropped === null ? "?" : dropped) +
        " framesReceived=" + (recv === null ? "?" : recv) +
        " jtarget=" + n(jtarget, 0) + "ms dec=" + n(dec, 2) + "ms" +
        // The panel's own refresh rate, and the session's frame rate as a multiple of it.
        // `js/refresh.js` has measured this since long before it mattered and it has never left
        // the browser — it feeds the fps picker as a hint and nothing else.
        //
        // Why it belongs here: a session rate that does not divide the panel rate evenly judders
        // while every other number on this line looks healthy, and a session rate *above* the
        // panel rate is frames that cannot physically be shown — bitrate, encode and decode spent
        // on nothing. Neither is visible in fps, decode time or loss. See the fps/refresh entry in
        // plan/TODO.md; this is its step 1, deliberately measurement-only.
        (W.refreshHz ? " hz=" + W.refreshHz + " ratio=" + (targetFps / W.refreshHz).toFixed(2) : "") +
        // **Whether anyone is looking at this page.** A backgrounded tab or a locked screen
        // still receives RTP and still counts `framesReceived`, but the browser decodes it
        // lazily — which reads in every other field as a decoder that has collapsed. Measured
        // 2026-09-13 12:02: decode went 11 ms -> 109 ms with the link flat, zero loss and 3 Mbps
        // arriving, and there was no way to tell a throttled tab from a hot phone. One field
        // settles it; see I18.
        " vis=" + (typeof document !== "undefined" ? document.visibilityState : "?") +
        (typeof document !== "undefined" && document.hasFocus && !document.hasFocus() ? " unfocused" : ""));
    }
    if (lost !== null) lastLost = lost;
  }, 1000);
};

W.stopStats = () => {
  if (W.statsTimer) { clearInterval(W.statsTimer); W.statsTimer = null; }
};
