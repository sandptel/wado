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

// How many render ticks in every N the compositor is sending. 1 = nothing shed.
//
// Without this the strip measures the effect of a mitigation it asked for and blames the sender:
// measured 2026-09-12 16:17:33, `bad the server — only 816 kbps arriving of 5.7 Mbps` about a
// frame rate this phone had requested three seconds earlier. Both the expected throughput and
// the per-frame decode budget scale with it.
let divisor = 1;
W.setShedding = (n) => { divisor = Math.max(1, +n || 1); };

// What the daemon says actually left its socket over the last stretch, kbps.
//
// **The discriminator this side cannot compute.** The viewer sees what arrived; it does not see
// what was sent, so "little is arriving and nothing was lost" is ambiguous between a sender that
// stopped and a path that is discarding silently — and the rule below guessed "the server" both
// times it was measured:
//
//   2026-09-12 22:33   5.35 Mbps sent   2.44 Mbps arrived   packetsLost 0   said "bad the server"
//   2026-09-13 01:19   5.13 Mbps sent   524 kbps arrived    packetsLost 0   said "bad the server"
//   2026-09-13 12:04     — sent —       91 kbps arrived     packetsLost 0   said "bad the server"
//
// Render pacing held 90/90 and the pump was clean through all three. `packetsLost = 0` does not
// mean no loss; it means that counter has nothing to say, and a rule that reads it as good news
// accuses whoever is left. `null` until the daemon reports — an absent reading must not be
// treated as zero, which would accuse the server even harder.
let sentKbps = null;
W.setSentKbps = (n) => {
  // `typeof`, not `+n`. **`+null` is 0**, and a zero here is not "no reading" — it is the
  // strongest possible accusation against the server, quoted as measured fact. Coercing an
  // absent value into one would make a missing report read as "the daemon sent nothing at all".
  // The wire value is a JSON number or it is not a reading.
  sentKbps = typeof n === "number" && Number.isFinite(n) && n >= 0 ? n : null;
};

W.setTargetKbps = (n) => {
  targetKbps = +n || 0;
  warm = 0;
  shown = { state: "ok", side: "healthy", detail: "", fix: "" };
  pending = null; pendingFor = 0; lastVerdict = "";
  // A new session is a new encoder and a new decoder, and the daemon has cleared its side.
  sentStrain = false;
  divisor = 1;
  sentKbps = null;
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

// How close to the offered frame rate counts as "keeping up". A decoder delivering this much of
// what it is sent is not saturated whatever its per-frame latency says — see the note on
// `keepingUp`. Ten percent of slack, because the two rates are sampled over different windows
// and an exact match is not something either counter promises.
const KEEPING_UP_FRAC = 0.9;
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

// Playout buffer above which the delay is worth naming. `js/webrtc.js` asks for 20 ms, and a
// settled link on this hardware sits near it; 40 is comfortably clear of normal variation while
// still catching the 47-54 ms a fresh connection starts at. A threshold rather than a trend
// because the user feels the level, not the slope.
const JBUF_WARN = 40;

// Last value sent to the daemon, so only changes go up the wire. Starts `false` to match the
// compositor, which clears `viewer_strained` both when a session starts and when a viewer
// rejoins a running one — so a healthy session sends nothing at all.
let sentStrain = false;
function reportStrain(strained) {
  // The frame-rate lock, in one line: the viewer keeps forming its verdict and keeps showing
  // it, but stops asking the compositor to act on it. Written here rather than at the call
  // site so *every* path into strain obeys it, including the `pageHidden` clear below.
  if (W.fpsLock) strained = false;
  if (strained === sentStrain) return;
  sentStrain = strained;
  if (W.relayStrain) W.relayStrain(strained);
}

// Is anyone actually looking at this page?
//
// **A hidden tab's numbers are not about the stream.** The browser stops pulling frames, so the
// jitter buffer balloons, the decoded rate wanders and the received bitrate collapses — and every
// rule here reads that as a fault. Measured live 2026-09-13 12:34:57, with the page hidden:
//
//     bad network — only 65 kbps arriving of 11.9 Mbps the server actually sent
//
// which is true as stated and completely the wrong conclusion: the bytes did arrive at the
// browser and the browser threw them away, because nobody was watching. Blaming the path for
// that sends the viewer to fix a network that was never broken, and — worse — a strain report
// from a hidden page sheds the compositor, so they come back to a reduced frame rate they never
// asked for.
//
// So the verdict is suspended while hidden rather than made cleverer. There is nothing useful to
// say about a stream nobody is receiving.
function pageHidden() {
  try {
    return typeof document !== "undefined" && document.visibilityState === "hidden";
  } catch (_) {
    return false;
  }
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
  // Nobody is looking — see `pageHidden`. Say so plainly and, crucially, **withdraw the strain
  // report**: a hidden page that leaves the flag set would have the compositor shedding for a
  // viewer that is not watching, and the viewer would return to a frame rate they never asked to
  // reduce. `warm` is rewound so the first seconds back are treated as ramp, which they are.
  if (pageHidden()) {
    reportStrain(false);
    warm = WARMUP_TICKS;
    emit({ type: "health", state: "ok", side: "not watching", detail: "", fix: "",
           needKbps: targetKbps || null, haveKbps: s.availableKbps, gotKbps: s.kbps });
    return;
  }
  const fps = s.fps;
  // The *effective* rate, not the requested one. At divisor 2 the phone is being sent 45 fps of
  // a 90 fps session and has 22 ms per frame, not 11 — judging it against the unshed budget would
  // keep it "saturated" forever and ratchet the shedding to the floor.
  const effFps = s.targetFps > 0 ? s.targetFps / divisor : 0;
  const budget = effFps > 0 ? 1000 / effFps : null;
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
  // Scaled by `divisor` for the same reason as the starvation rule below: under a shed, "enough
  // is arriving" means enough for the rate actually being sent.
  const arriving = gotKbps !== null && targetKbps > 0 &&
                   gotKbps >= (targetKbps / divisor) * TRUST_DECODER_FRAC;
  if (arriving) {
    if (s.decodeDropPct !== null && s.decodeDropPct >= DEVICE_DROP_WARN) {
      worse("warn", "your device", s.decodeDropPct.toFixed(1) + "% of frames dropped after arriving");
    }
    // **Decode time is latency, not capacity.** A serial decoder that takes longer than a frame
    // interval per frame cannot keep up, and that is the assumption this rule was written on. A
    // *pipelined* hardware decoder breaks it: it can hold 50 ms of per-frame latency while
    // sustaining 90 frames a second across several threads, and reading that as saturation makes
    // the strip accuse a phone that is keeping up perfectly.
    //
    // Measured live 2026-09-13 12:23:03 — `bad your device, decode 16.4 ms against 11.1 ms`,
    // which reported strain and shed the compositor to 1-in-2, while the same snapshot said
    // `fps=90` against a target of 90 with `framesDropped` flat at 134. We halved the frame rate
    // of a decoder that was not behind by a single frame. `watch.sh` had already spotted the
    // ambiguity and said so in as many words — *"past capacity, or decoding on several
    // threads"* — but the strip never learned to tell them apart.
    //
    // So saturation now needs evidence of actually falling behind, and the snapshot already
    // carries both kinds: the decoded frame rate, and frames dropped after arriving. The case
    // this whole mechanism was built for — a phone decoding 15 of the 90 frames a second it is
    // sent — fails `keepingUp` on the first term and still fires.
    const keepingUp =
      fps !== null && effFps > 0 && fps >= effFps * KEEPING_UP_FRAC &&
      (s.decodeDropPct === null || s.decodeDropPct < DEVICE_DROP_WARN);
    if (!keepingUp && s.dec !== null && budget !== null && s.dec >= budget * DECODE_BUDGET_FRAC) {
      worse(s.dec >= budget ? "bad" : "warn", "your device",
            "decode " + s.dec.toFixed(1) + " ms against a " + budget.toFixed(1) + " ms budget");
    }
  }

  // — server — nothing is arriving and nothing was lost, so it was never sent. Checked last so
  // it cannot mask a network fault that explains the same symptom.
  //
  // `divisor` is what keeps this honest. A shed session legitimately delivers a fraction of both
  // the bitrate and the frame rate, and every term here would otherwise read as the compositor
  // having stopped — which is the accusation that fired at 16:17:33 about a shed the phone had
  // asked for. Both thresholds scale, so this can still catch a genuinely dead sender underneath
  // an active shed.
  const expectKbps = targetKbps / divisor;
  if (state === "ok" && s.lossPct !== null && s.lossPct < LOSS_WARN &&
      gotKbps !== null && expectKbps > 0 && gotKbps < expectKbps * STARVED_FRAC &&
      fps !== null && effFps > 0 && fps < effFps * 0.8) {
    const shed = divisor > 1 ? " (sending 1 tick in " + divisor + ")" : "";
    // Three answers where there used to be one, and `sentKbps` is what separates them.
    if (sentKbps !== null && sentKbps > expectKbps * STARVED_FRAC) {
      // The daemon sent it and it did not arrive. Nothing on this phone caused that and nothing
      // on this phone fixes it — the bytes went into the path and did not come out.
      worse("bad", "network",
            "only " + mbps(gotKbps) + " arriving of " + mbps(sentKbps) +
            " the server actually sent — the path is discarding it silently" + shed);
    } else if (sentKbps !== null) {
      // The daemon says so itself: it is not sending. Now the accusation is evidence.
      worse("bad", "the server",
            "the server sent only " + mbps(sentKbps) + " of " + mbps(expectKbps) + " asked for" + shed);
    } else {
      // No report yet — an older daemon, or the first stretch of a new session. Say that the
      // number is missing rather than pretending the verdict is as firm as the two above.
      worse("bad", "the server",
            "only " + mbps(gotKbps) + " arriving of " + mbps(expectKbps) +
            " asked for, none lost (the server has not reported what it sent)" + shed);
    }
  }

  // ⚠️ **Checked after the server rule, not before it, and the harness is why.** The server
  // rule is guarded by `state === "ok"` so it cannot mask a network fault; written above it,
  // this rule set `warn` first and silently suppressed a dead-sender verdict entirely.
  // `scripts/health-check.mjs` caught it on the first run. Placed here, `worse()`'s ranking
  // does the right thing on its own: a real fault is `bad` and outranks this.
  // — the buffer between the two ends, which is latency nothing else here can see.
  //
  // Measured 2026-09-13 across the two minutes after a reconnect: `jbuf` 47 -> 28 ms while
  // `fps` held 116-121, `rtt` sat at 24-37, `lost` was 0 and `framesDropped` never moved. Every
  // rule above and below said healthy, and the picture was six frame periods behind the finger
  // at 120 fps. That is what the user reported as "input does not feel synced with fps", and it
  // was invisible because every other metric is about *rate* and this one is about *delay*.
  //
  // Reported, not fixed — and deliberately so. `js/webrtc.js` already sets the playout hint to
  // 20 ms and already carries the note explaining why re-asserting it does nothing: the hint is
  // a floor, the browser's own timing model outranks it, and here `jtarget` reads 49-54 against
  // our 20, which is that being outranked in plain sight. The buffer drains on its own as the
  // model learns the path is steady. So: say what is happening and that it clears, rather than
  // claim health or offer a fix that does not exist.
  //
  // `warn`, never `bad`, and never a strain report: nothing is wrong with the phone, nothing is
  // wrong with the link, and shedding the frame rate would not shorten a queue that is being
  // held for jitter rather than filled by congestion. See plan/sync.md §1b.
  if (s.jbuf !== null && s.jbuf >= JBUF_WARN) {
    worse("warn", "settling", "the picture is about " + s.jbuf.toFixed(0) +
          " ms behind while the connection settles — this clears on its own");
  }

  // What to do about it. One suggestion per side, and none while healthy — advice offered
  // when nothing is wrong is noise that teaches the reader to skip the line it sits on.
  let fix = "";
  if (state !== "ok" && s.targetFps > 0) {
    const lower = effFps > 60 ? 60 : 30;
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
