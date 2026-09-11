// wado bridge — latency breakdown.
//
// One job: collect per-stage latency numbers from the three places that can honestly
// measure them, and emit them as one record for the UI.
//
// There is deliberately NO single glass-to-glass figure. The browser and the server have
// no shared clock, so any fused end-to-end number would be invented. What is real:
//
//   server  capture / encode / queue   → polled from GET /timing
//   network net                        → half the ICE candidate-pair RTT
//   client  buf / decode               → getStats() on the inbound video stream
//   input   in                         → round trip of a Ping we echo off the server
//
// The input figure is the one that matters for "dragging feels slow": it is measured on
// the same reliable data channel real buttons and keys use, so a backed-up channel shows
// up here immediately.

const PING_EVERY_MS = 500; // input probe rate — cheap (a few bytes), no need to flood

W.latency = {
  _timer: null,
  _pingTimer: null,
  _seq: 0,
  // seq -> performance.now() at send, for outstanding probes.
  _inflight: new Map(),
  _inMs: null,

  // Called by the server's pong echo (wired in W.attachLatencyEcho).
  onPong(seq) {
    const sent = this._inflight.get(seq);
    if (sent === undefined) return;
    this._inflight.delete(seq);
    const rtt = performance.now() - sent;
    // Smooth lightly: a single SCTP round trip is spiky, and a wandering number is
    // harder to read than a slightly laggy one.
    this._inMs = this._inMs == null ? rtt : this._inMs * 0.7 + rtt * 0.3;
  },

  // Hook for W.sendInput — currently unused, kept so the send path can grow a per-event
  // timestamp later without touching input_core again.
  onInputSent(_obj) {},

  start(pc) {
    this.stop();
    this._inflight.clear();
    this._inMs = null;

    this._pingTimer = setInterval(() => {
      const dc = W.inputDC;
      if (!dc || dc.readyState !== "open") return;
      const seq = ++this._seq & 0xffff;
      this._inflight.set(seq, performance.now());
      // Drop probes that never came back, so the map can't grow without bound.
      if (this._inflight.size > 16) {
        const oldest = this._inflight.keys().next().value;
        this._inflight.delete(oldest);
      }
      try { dc.send(JSON.stringify({ t: "ping", seq })); } catch (_) {}
    }, PING_EVERY_MS);

    this._timer = setInterval(async () => {
      if (!W.pc || W.pc !== pc) { this.stop(); return; }

      // --- client + network legs, from the browser's own stats ---
      let net = null, buf = null, decode = null;
      try {
        const stats = await pc.getStats();
        stats.forEach((r) => {
          if (r.type === "inbound-rtp" && (r.kind === "video" || r.mediaType === "video")) {
            if (typeof r.jitterBufferDelay === "number" &&
                typeof r.jitterBufferEmittedCount === "number" &&
                r.jitterBufferEmittedCount > 0) {
              buf = (r.jitterBufferDelay / r.jitterBufferEmittedCount) * 1000;
            }
            if (typeof r.totalDecodeTime === "number" &&
                typeof r.framesDecoded === "number" && r.framesDecoded > 0) {
              decode = (r.totalDecodeTime / r.framesDecoded) * 1000;
            }
          } else if (r.type === "candidate-pair" && (r.nominated || r.state === "succeeded")) {
            // One-way estimate; RTT is the only thing actually observable.
            if (typeof r.currentRoundTripTime === "number") net = (r.currentRoundTripTime * 1000) / 2;
          }
        });
      } catch (_) {}

      // --- server legs ---
      let srv = null;
      try {
        const resp = await fetch(W.server + "/timing", { cache: "no-store" });
        if (resp.ok) srv = await resp.json();
      } catch (_) {}

      emit({
        type: "latency",
        capture: srv ? srv.capture_ms : null,
        encode: srv ? srv.encode_ms : null,
        queue: srv ? srv.queue_ms : null,
        tick: srv ? srv.tick_ms : null,
        dropped: srv ? srv.dropped : null,
        net,
        buf,
        decode,
        input: this._inMs,
      });
    }, 1000);
  },

  stop() {
    if (this._timer) { clearInterval(this._timer); this._timer = null; }
    if (this._pingTimer) { clearInterval(this._pingTimer); this._pingTimer = null; }
  },
};

// Debug toggle. This module owns the timers, so it owns the switch that stops them — the
// toggle used to gate only the *display*, leaving a 2 Hz ping on the input data channel and a
// 1 Hz /timing fetch running for numbers nothing rendered. That is non-input traffic on the
// channel invariant #1 exists to protect, so "off" now means off.
W.debugLatency = false;
W.setLatency = (on) => {
  W.debugLatency = !!on;
  if (W.debugLatency && W.pc) W.latency.start(W.pc);
  else W.latency.stop();
};

// The server answers our Ping on the same channel it arrived on. Attach once the reliable
// input channel exists; non-pong traffic is ignored (the server sends nothing else).
W.attachLatencyEcho = () => {
  const dc = W.inputDC;
  if (!dc || dc._wadoEchoAttached) return;
  dc._wadoEchoAttached = true;
  dc.addEventListener("message", (ev) => {
    try {
      const m = JSON.parse(ev.data);
      if (m && m.t === "pong") W.latency.onPong(m.seq);
    } catch (_) {}
  });
};
