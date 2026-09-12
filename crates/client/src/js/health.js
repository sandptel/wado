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
W.setTargetKbps = (n) => {
  targetKbps = +n || 0;
  warm = 0;
  shown = { state: "ok", side: "healthy", detail: "", fix: "" };
  pending = null; pendingFor = 0; lastVerdict = "";
  // A new session is a new encoder and a new decoder, and the daemon has cleared its side.
  sentStrain = false;
};

// Ticks to ignore after a session starts. See the note on `warm` at the first use.
const WARMUP_TICKS = 5;
let warm = 0;

// Consecutive ticks a new verdict must survive before it is shown.
//
// Without this the strip flickered green/amber/red once a second, because a decode time sitting
// on its budget (16.1, 17.4, 15.7, 24.7, 16.6 ms against 16.7) crosses the threshold every
// tick. A readout that changes colour every second is not a diagnosis; it is a distraction, and
// it relayed a log line each time too. Three ticks is enough to ride out single-sample noise
// and still react inside five seconds.
const SETTLE_TICKS = 3;
let shown = { state: "ok", side: "healthy", detail: "", fix: "" };
let pending = null, pendingFor = 0;

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
// Below this fraction of the CBR target actually arriving, `dec` says nothing about the phone.
// A decoder waiting on packets that never came reads exactly like one that cannot keep up — same
// long decode times, same dropped frames — and blaming the phone for a broken path sends the
// viewer to change settings that were never the problem. `scripts/watch.sh` has enforced this on
// the server side since the run of 2026-09-12; this is the same rule on the side that has the
// numbers first.
const TRUST_DECODER_FRAC = 0.6;

// Last value sent to the daemon, so only changes go up the wire. Starts `false` to match the
// compositor, which clears `viewer_strained` both when a session starts and when a viewer
// rejoins a running one — so a healthy session sends nothing at all.
let sentStrain = false;
function reportStrain(strained) {
  if (strained === sentStrain) return;
  sentStrain = strained;
  if (W.relayStrain) W.relayStrain(strained);
}

// `s` is the snapshot stats.js already computed; extras are the fields only this file reads.
W.health = (s) => {
  // Warm-up: the first seconds of a connection are ramp, not steady state, and every rule here
  // reads a one-second rate. Report healthy rather than nothing, so the strip still appears.
  if (warm++ < WARMUP_TICKS) {
    emit({ type: "health", state: "ok", side: "connecting", detail: "", fix: "",
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
  // ⛔ There is deliberately no rule on `availableIncomingBitrate`. It was tried twice and it
  // lies in both directions: while nothing is congested Chrome tracks the *received* rate with
  // it, so a static screen reports a tiny "link"; and measured here on 2026-09-12 it reported
  // 123 kbps while 9.4 Mbps was demonstrably flowing, zero loss, 60/60 fps. A field that can be
  // off by a factor of seventy-six is not evidence about anything. It stays on the strip as a
  // figure to look at and has no vote.
  //
  // Nothing is lost by that: a link genuinely too small for the stream shows up as loss or as a
  // frame-rate shortfall, and both already have rules above.

  // — device — it all arrived; this phone cannot keep up with it. "It all arrived" is the
  // load-bearing half: without `arriving` every receiver number below is about a stream that was
  // never delivered, and the verdict accuses the phone for a fault on the path.
  const arriving = gotKbps !== null && targetKbps > 0 && gotKbps >= targetKbps * TRUST_DECODER_FRAC;
  if (arriving) {
    if (s.decodeDropPct !== null && s.decodeDropPct >= DEVICE_DROP_WARN) {
      worse("warn", "your device", s.decodeDropPct.toFixed(1) + "% of frames dropped after arriving");
    }
    if (s.dec !== null && budget !== null && s.dec >= budget * DECODE_BUDGET_FRAC) {
      worse(s.dec >= budget ? "bad" : "warn", "your device",
            "decode " + s.dec.toFixed(1) + " ms against a " + budget.toFixed(1) + " ms budget");
    }
  }

  // — server — nothing is arriving and nothing was lost, so it was never sent. Checked last so
  // it cannot mask a network fault that explains the same symptom.
  if (state === "ok" && s.lossPct !== null && s.lossPct < LOSS_WARN &&
      gotKbps !== null && targetKbps > 0 && gotKbps < targetKbps * STARVED_FRAC &&
      fps !== null && s.targetFps > 0 && fps < s.targetFps * 0.8) {
    worse("bad", "the server",
          "only " + mbps(gotKbps) + " arriving of " + mbps(targetKbps) + " asked for, none lost");
  }

  // What to do about it. One suggestion per side, and none while healthy — advice offered
  // when nothing is wrong is noise that teaches the reader to skip the line it sits on.
  let fix = "";
  if (state !== "ok" && s.targetFps > 0) {
    const lower = s.targetFps > 60 ? 60 : 30;
    // Nothing deliberately for "the server": no setting on this phone fixes a compositor that
    // stopped producing, and offering one would send the viewer to change things at random.
    if (side === "your device") fix = "try " + lower + " fps";
    else if (side === "network") fix = "try " + lower + " fps or a smaller resolution";
  }

  // Hysteresis. A verdict has to hold for SETTLE_TICKS before it replaces the one on screen;
  // the numbers behind the *current* verdict are refreshed every tick regardless, so the strip
  // stays live without changing its mind.
  if (state === shown.state && side === shown.side) {
    shown = { state, side, detail, fix };
    pending = null; pendingFor = 0;
  } else if (pending && pending.state === state && pending.side === side) {
    if (++pendingFor >= SETTLE_TICKS) { shown = { state, side, detail, fix }; pending = null; pendingFor = 0; }
    else pending = { state, side, detail, fix };
  } else {
    pending = { state, side, detail, fix }; pendingFor = 1;
  }

  state = shown.state; side = shown.side; detail = shown.detail; fix = shown.fix;

  // Tell the server, so it can do something about it rather than only advising the viewer to.
  //
  // Keyed on the **settled** verdict and nothing else. It used to be `saturated && side === ...`,
  // mixing this tick's decode reading with the settled side, and that oscillated in the field on
  // 2026-09-12 at 21:43: a decode time sitting near its budget crosses it every second or two, so
  // the flag flipped about every 1.5 s and the compositor walked 1 -> 2 -> 1 -> 2 for a minute.
  // Shedding that oscillates is the failure `crates/compositor/src/congestion.rs` was written to
  // avoid, and `SETTLE_TICKS` existed to prevent it two lines above — the bug was reaching past it.
  //
  // `side === "your device"` is sufficient on its own: both device rules live inside the
  // `arriving` branch, so the settled side cannot be "your device" unless the stream was also
  // genuinely turning up. Sent on change only — it is a level the daemon latches.
  reportStrain(side === "your device");

  emit({ type: "health", state, side, detail, fix,
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
           (fix ? "  (" + fix + ")" : "") +
           (targetKbps ? "  [got " + mbps(gotKbps || 0) + " of " + mbps(targetKbps) +
            (haveKbps ? ", link " + mbps(haveKbps) : "") + "]" : ""));
  }
};

let lastVerdict = "";

function mbps(k) {
  return k >= 1000 ? (k / 1000).toFixed(1) + " Mbps" : Math.round(k) + " kbps";
}
