// wado bridge — the one-line verdict: *whose* fault is this?
//
// Separate from `stats.js` on purpose. That file's job is reading `getStats()` honestly; this
// file's job is turning the reading into an accusation, and the two change for different
// reasons — a new WebRTC counter touches only the reader, a better rule touches only this.
//
// The accusation matters because the three faults look identical from the sofa (the picture
// stutters) and have nothing in common underneath:
//
//   * **network** — packets are being lost or delayed between the two ends. Nothing on either
//     machine is wrong. Moving to better signal is the fix.
//   * **device** — the packets arrived; this phone cannot decode them fast enough. Lower fps
//     or resolution is the fix.
//   * **server** — nothing is arriving, and nothing is lost either, so the far end never sent
//     it. The compositor, encoder or the machine's CPU is the fix, and it is the only one of
//     the three the user cannot do anything about from here.
//
// The discriminator is **loss against throughput**: loss high ⇒ network; loss zero *and*
// throughput far under target ⇒ sender; everything arriving but frames dropped or decode over
// budget ⇒ receiver. That is the whole of it, and it is why `availableIncomingBitrate` is read
// too: "needs 8 Mbps, link offers 1.4" turns "network" from a label into a number.

// What the session asked the encoder for, in kbps. Set when a session starts — without it the
// bandwidth half can say what is arriving but not whether that is enough.
let targetKbps = 0;
W.setTargetKbps = (n) => { targetKbps = +n || 0; warm = 0; };

// Ticks to ignore after a session starts. See the note on `warm` at the first use.
const WARMUP_TICKS = 5;
let warm = 0;

// Loss this bad is a broken path, not a blip. Below it a stream recovers by itself.
const LOSS_WARN = 0.5, LOSS_BAD = 3.0;       // percent of packets in the window
const RTT_WARN = 150, RTT_BAD = 300;          // ms
const JITTER_WARN = 30;                       // ms
const DEVICE_DROP_WARN = 2.0;                 // percent of frames dropped after arrival
// Decode has no headroom left at the frame budget; past this a backlog can never drain.
const DECODE_BUDGET_FRAC = 0.9;
// Arriving at less than this fraction of what was asked for, with nothing lost, means the far
// end never sent it. Generous, because a still screen legitimately encodes to almost nothing —
// which is why `fps` has to be low as well before this fires.
const STARVED_FRAC = 0.25;

// `s` is the snapshot stats.js already computed; extras are the fields only this file reads.
W.health = (s) => {
  // Warm-up: the first seconds of a connection are ramp, not steady state, and every rule here
  // reads a one-second rate. Report healthy rather than nothing, so the strip still appears.
  if (warm++ < WARMUP_TICKS) {
    emit({ type: "health", state: "ok", side: "connecting", detail: "",
           needKbps: targetKbps || null, haveKbps: s.availableKbps, gotKbps: s.kbps });
    return;
  }
  const fps = s.fps, budget = s.targetFps > 0 ? 1000 / s.targetFps : null;
  const haveKbps = s.availableKbps;          // link capacity the browser estimates
  const gotKbps = s.kbps;                    // what the video track is actually receiving
  let state = "ok", side = "healthy", detail = "";

  const worse = (st, sd, d) => {
    const rank = { ok: 0, warn: 1, bad: 2 };
    if (rank[st] > rank[state]) { state = st; side = sd; detail = d; }
  };

  // — network — loss and delay between the two ends.
  if (s.lossPct !== null && s.lossPct >= LOSS_WARN) {
    worse(s.lossPct >= LOSS_BAD ? "bad" : "warn", "network",
          s.lossPct.toFixed(1) + "% packet loss");
  }
  if (s.ping !== null && s.ping >= RTT_WARN) {
    worse(s.ping >= RTT_BAD ? "bad" : "warn", "network", "round trip " + s.ping.toFixed(0) + " ms");
  }
  if (s.jitter !== null && s.jitter >= JITTER_WARN) {
    worse("warn", "network", "jitter " + s.jitter.toFixed(0) + " ms");
  }
  // A link that cannot carry the stream is a network fault even with zero loss today: the
  // encoder is about to be told to back off, or the queue is about to grow.
  const suffering = (fps !== null && s.targetFps > 0 && fps < s.targetFps * 0.9) ||
                    (s.lossPct !== null && s.lossPct >= LOSS_WARN);
  if (suffering && haveKbps !== null && targetKbps > 0 && haveKbps < targetKbps) {
    worse(haveKbps < targetKbps / 2 ? "bad" : "warn", "network",
          "link offers " + mbps(haveKbps) + ", stream wants " + mbps(targetKbps));
  }

  // — device — it all arrived; this phone cannot keep up with it.
  if (s.decodeDropPct !== null && s.decodeDropPct >= DEVICE_DROP_WARN) {
    worse("warn", "your device", s.decodeDropPct.toFixed(1) + "% of frames dropped after arriving");
  }
  if (s.dec !== null && budget !== null && s.dec >= budget * DECODE_BUDGET_FRAC) {
    worse(s.dec >= budget ? "bad" : "warn", "your device",
          "decode " + s.dec.toFixed(1) + " ms against a " + budget.toFixed(1) + " ms budget");
  }

  // — server — nothing is arriving and nothing was lost, so it was never sent. Checked last so
  // it cannot mask a network fault that explains the same symptom.
  if (state === "ok" && s.lossPct !== null && s.lossPct < LOSS_WARN &&
      gotKbps !== null && targetKbps > 0 && gotKbps < targetKbps * STARVED_FRAC &&
      fps !== null && s.targetFps > 0 && fps < s.targetFps * 0.8) {
    worse("bad", "the server",
          "only " + mbps(gotKbps) + " arriving of " + mbps(targetKbps) + " asked for, none lost");
  }

  emit({ type: "health", state, side, detail,
         needKbps: targetKbps || null, haveKbps, gotKbps });

  // On change only, up the relay to the daemon log. Two reasons, and the second is the one
  // that matters: it puts the *client's own conclusion* next to the server's numbers in one
  // file, so a session can be diagnosed afterwards without a human having been watching; and
  // it is the only way to tell, from outside the phone, that the verdict is being computed at
  // all — a strip that never renders and a strip that renders "healthy" look identical from
  // here. On change only, because at 1 Hz this is a log line per second per viewer.
  const now = state + "/" + side;
  if (now !== lastVerdict) {
    lastVerdict = now;
    if (W.rlog) W.rlog("verdict " + state + " " + side + (detail ? " — " + detail : "") +
           (targetKbps ? "  [got " + mbps(gotKbps || 0) + " of " + mbps(targetKbps) +
            (haveKbps ? ", link " + mbps(haveKbps) : "") + "]" : ""));
  }
};

let lastVerdict = "";

function mbps(k) {
  return k >= 1000 ? (k / 1000).toFixed(1) + " Mbps" : Math.round(k) + " kbps";
}
