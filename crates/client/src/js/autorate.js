// wado bridge — auto bitrate: cap the encoder to what this link actually carries.
//
// The server cannot sense the link; its only lever was shedding frames (compositor congestion),
// which on a link too small for the stream still left the picture arriving in bursts and the
// browser buffering a quarter second to smooth them (measured 2026-10-03: 3.9 Mbps sent into a
// ~0.5–1.9 Mbps cellular path, rtt 24→360 ms, jbuf ~270 ms, 22 of 90 fps). This lowers the
// encoder's rate instead, through a session reconfigure that rebuilds only the encoder.
//
// Signals: round trip against its own recent floor and loss decide; the rate that actually
// arrived sizes the step. Not `availableIncomingBitrate`: measured off by up to 76× (health.js).
//
//   down  3 congested seconds → ~90% of the best rate that got through, at most halving
//   up    10 clean seconds   → +50%, until the full rate (the cap is then lifted)
//   any change at most every 6 s, so a step can show its effect before the next
const MIN_KBPS = 400, DOWN_AFTER = 3, UP_AFTER = 10, GAP_MS = 6000;

W.autorate = {
  enabled: true,
  reset() {
    this.cap = null;       // kbps in force, or null for the full rate
    this.ceil = 0;         // the full rate, as the server last reported it uncapped
    this.rtts = [];        // last 30 s, for the floor
    this.recent = [];      // last 5 s, whose median decides
    this.recv = [];        // last 5 s of received kbps
    this.bad = 0; this.good = 0; this.last = 0;
  },
  // The server's rate after a start or reconfigure (health.js). Uncapped, it is the ceiling.
  target(kbps) { if (this.cap === null && kbps > 0) this.ceil = kbps; },
  // One stats sample a second (stats.js). Returns the new cap when it changes, for the check.
  feed(s, now = Date.now()) {
    if (!this.enabled || !this.ceil) return undefined;
    if (s.ping != null) { this.rtts.push(s.ping); if (this.rtts.length > 30) this.rtts.shift(); }
    if (s.kbps != null) { this.recv.push(s.kbps); if (this.recv.length > 5) this.recv.shift(); }
    const floor = this.rtts.length ? Math.min(...this.rtts) : 0;
    const loss = s.lossPct || 0;
    // Round trip over its floor, or loss: a path holding more than it can carry. Not the playout
    // buffer — measured 2026-10-03, jbuf sat at 300–400 ms with rtt at its floor and no loss
    // (arrival jitter, not capacity), and cutting the bitrate for it changed nothing.
    //
    // The median of the last 5 samples, against a floor of at least 20 ms: a jittery link with a
    // tiny floor (5 ms measured 2026-10-03) cleared "floor + 60" on single spikes, and the cap
    // walked 2521 → 400 kbps with the buffers not moving at all — jitter, not capacity.
    this.recent = [...(this.recent || []), s.ping].filter((v) => v != null).slice(-5);
    const mid = this.recent.length ? [...this.recent].sort((a, b) => a - b)[this.recent.length >> 1] : null;
    const congested = (mid != null && mid > Math.max(floor, 20) + 60) || loss > 2;
    const clean = !congested && (s.ping == null || s.ping <= floor + 25) && loss < 0.5 &&
                  (s.jbuf == null || s.jbuf < 80);
    this.bad = congested ? this.bad + 1 : 0;
    this.good = clean ? this.good + 1 : 0;
    if (now - this.last < GAP_MS) return undefined;
    const cur = this.cap || this.ceil;
    let next;
    if (this.bad >= DOWN_AFTER) {
      const got = Math.max(...this.recv, 0);
      next = Math.max(MIN_KBPS, Math.round(Math.max(cur * 0.5, Math.min(cur * 0.7, got * 0.9))));
      if (next >= cur) return undefined;
    } else if (this.good >= UP_AFTER && this.cap !== null) {
      next = Math.round(cur * 1.5);
      if (next >= this.ceil * 0.95) next = null;   // back to the full rate
    } else {
      return undefined;
    }
    this.cap = next; this.last = now; this.bad = 0; this.good = 0;
    if (typeof emit === "function") emit({ type: "autorate", kbps: next });
    if (W.rlog) W.rlog(`autorate ${next === null ? "full rate " + this.ceil : next} kbps (was ${cur}; rtt floor ${floor.toFixed(0)} ms)`);
    return next;
  },
};
W.autorate.reset();
{
  const set = W.setTargetKbps;
  W.setTargetKbps = (n) => { W.autorate.target(n); if (set) set(n); };
}
