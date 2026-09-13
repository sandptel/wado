// Runnable check for js/health.js — the verdict rules, and nothing else.
//
// It exists because the rules are the one part of the health strip that can be *wrong* rather
// than merely ugly: accusing the network when the phone cannot decode sends someone to move
// closer to a router that was never the problem. Each case below is a fault whose symptoms
// overlap with another's, so a rule that collapses two of them fails here.
//
// Run:  node scripts/health-check.mjs
import { readFileSync } from "node:fs";

const src = readFileSync(new URL("../crates/client/src/js/health.js", import.meta.url), "utf8");
let out = null;
const W = {};
let logged = [];
let strains = [];
W.rlog = (l) => logged.push(l);   // the bridge supplies this; here it is captured
W.relayStrain = (b) => strains.push(b);   // what would go up the wire to the daemon
const emit = (m) => { out = m; };
new Function("W", "emit", src)(W, emit);

// See the note on `dup` at the bottom: a session boundary must break the repeat comparison.
const SESSION_MARK = "--- new session ---";
const rawSetTargetKbps = W.setTargetKbps;
W.setTargetKbps = (n) => { logged.push(SESSION_MARK); rawSetTargetKbps(n); };

let failures = 0;
// `sent` is what the daemon reports it actually put on the wire, kbps — `undefined` means it has
// not said. It is applied *after* `setTargetKbps`, because starting a session clears the last
// session's figure (correctly: it belongs to an encoder that no longer exists), and setting it
// before would be silently wiped. That ordering ate two cases the first time these were written.
const check = (name, target, snapshot, wantSide, wantState, sent) => {
  W.setTargetKbps(target.kbps);        // also resets the warm-up, the shed divisor and `sentKbps`
  if (sent !== undefined) W.setSentKbps(sent);
  const s = { fps: null, ping: null, jbuf: null, dec: null, jitter: null, kbps: null,
              lossPct: null, decodeDropPct: null, availableKbps: null,
              targetFps: target.fps, ...snapshot };
  for (let i = 0; i < 9; i++) W.health(s);   // warm-up, then let the verdict settle
  const ok = out.side === wantSide && out.state === wantState;
  // A healthy stream never carries a suggestion, and a fault the *viewer* can do something
  // about always does. "the server" deliberately carries none: no setting on this phone fixes
  // a compositor that stopped producing frames, and offering one would be a lie.
  const wantFix = out.state !== "ok" && out.side !== "the server";
  if (ok && wantFix !== (out.fix !== "")) {
    failures++; console.log(`FAIL ${name}: fix=${JSON.stringify(out.fix)} for ${out.state}/${out.side}`); return;
  }
  if (!ok) { failures++; console.log(`FAIL ${name}: got ${out.state}/${out.side} "${out.detail}", want ${wantState}/${wantSide}`); }
  else console.log(`ok   ${name}  →  ${out.state}/${out.side}  ${out.detail}${out.fix ? "  [" + out.fix + "]" : ""}`);
};

const T = { kbps: 8000, fps: 90 };          // a typical session: 8 Mbps at 90 fps
const WARMUP = 5;                           // must match WARMUP_TICKS in health.js

check("clean stream", T,
  { fps: 90, ping: 27, dec: 4.0, jitter: 3, kbps: 7800, lossPct: 0.0, decodeDropPct: 0.0, availableKbps: 20000 },
  "healthy", "ok");

check("lossy path", T,
  { fps: 70, ping: 40, dec: 4.0, jitter: 8, kbps: 4000, lossPct: 5.2, decodeDropPct: 0, availableKbps: 20000 },
  "network", "bad");

// The phone received everything and could not decode it. Loss is zero, so a loss-based rule
// would call this healthy and a throughput-based one would blame the server.
check("phone cannot decode", T,
  { fps: 88, ping: 30, dec: 12.0, jitter: 4, kbps: 7800, lossPct: 0.0, decodeDropPct: 6.0, availableKbps: 20000 },
  "your device", "bad");

// Nothing lost, nothing arriving, frame rate on the floor: the far end never sent it.
check("server stopped producing", T,
  { fps: 20, ping: 25, dec: 3.0, jitter: 3, kbps: 300, lossPct: 0.0, decodeDropPct: 0, availableKbps: 20000 },
  "the server", "bad");

// A link too small for the stream is a network fault *before* any packet is lost.
// availableIncomingBitrate has no vote — see the note in health.js. Measured live at 123 kbps
// while 9.4 Mbps was flowing with zero loss at 60/60 fps, so a rule keyed on it accuses the
// network while the stream is perfect. Both directions of that error are pinned here.
check("absurd link estimate does not accuse the network", T,
  { fps: 90, ping: 30, dec: 4.0, jitter: 4, kbps: 7800, lossPct: 0.0, decodeDropPct: 0, availableKbps: 123 },
  "healthy", "ok");

check("a genuinely small link still shows up, as loss", T,
  { fps: 60, ping: 30, dec: 4.0, jitter: 4, kbps: 1300, lossPct: 4.0, decodeDropPct: 0, availableKbps: 1400 },
  "network", "bad");

// A still screen encodes to almost nothing. Throughput alone must never accuse the server.
check("idle screen is not a fault", T,
  { fps: 90, ping: 27, dec: 3.0, jitter: 3, kbps: 60, lossPct: 0.0, decodeDropPct: 0, availableKbps: 20000 },
  "healthy", "ok");

// ── The decoder can only be blamed for a stream that reached it ──────────────────────────────
//
// Observed live 2026-09-12 20:36:56: the strip read "bad your device — decode 48.7 ms" while
// 69 kbps of 5.7 Mbps was arriving. That is a decoder idling between packets, not one drowning,
// and the monitor said UNCLEAR about the very same sample. The client had the numbers first and
// got it wrong.
check("a starved decoder is not the phone's fault", T,
  { fps: 12, ping: 30, dec: 48.7, jitter: 6, kbps: 400, lossPct: 0.0, decodeDropPct: 30.0,
    availableKbps: 20000 },
  "the server", "bad");

// ── What gets sent to the daemon ─────────────────────────────────────────────────────────────
{
  const run = (target, snapshot, ticks = 9) => {
    W.setTargetKbps(target.kbps);
    const s = { fps: null, ping: null, jbuf: null, dec: null, jitter: null, kbps: null,
                lossPct: null, decodeDropPct: null, availableKbps: null,
                targetFps: target.fps, ...snapshot };
    for (let i = 0; i < ticks; i++) W.health(s);
  };
  const want = (name, got, expected) => {
    const ok = JSON.stringify(got) === JSON.stringify(expected);
    if (!ok) { failures++; console.log(`FAIL ${name}: got ${JSON.stringify(got)}, want ${JSON.stringify(expected)}`); }
    else console.log(`ok   ${name}`);
  };

  // Saturated and everything arriving: report once, not once per tick. The daemon latches it.
  strains = [];
  run(T, { fps: 88, ping: 30, dec: 12.0, jitter: 4, kbps: 7800, lossPct: 0.0, decodeDropPct: 6.0,
           availableKbps: 20000 });
  want("strain is reported once, not every tick", strains, [true]);

  // The same decode time on a starved stream must send nothing at all — not even `false`.
  // Both sides start at "not strained", so silence is the agreement; a message here would be
  // noise on the very link that is already the problem. And shedding would make it worse, by
  // cutting the frame rate of a stream that is not arriving in the first place.
  strains = [];
  run(T, { fps: 12, ping: 30, dec: 48.7, jitter: 6, kbps: 400, lossPct: 0.0, decodeDropPct: 30.0,
           availableKbps: 20000 });
  want("a starved decoder reports no strain", strains, []);

  // ── The frame-rate lock (plan/sync.md §1) ──────────────────────────────────────────────────
  //
  // What it must do: stop the compositor's rate from moving. What it must NOT do: stop the
  // viewer forming and showing a verdict — the strip still says the phone is struggling, it
  // just stops asking anyone to act on it.
  strains = [];
  W.fpsLock = true;
  run(T, { fps: 88, ping: 30, dec: 12.0, jitter: 4, kbps: 7800, lossPct: 0.0, decodeDropPct: 6.0,
           availableKbps: 20000 });
  want("locked: a strained viewer asks for no shed", strains, []);
  want("locked: the verdict is still formed and shown", [out.state, out.side], ["bad", "your device"]);
  W.fpsLock = false;

  // The case the one-line guard exists for, and the reason it lives inside `reportStrain`
  // rather than at the call site: ticking the box *while already shedding* must release the
  // latch. The daemon holds `viewer_strained` until told otherwise, so a lock that only
  // suppressed future `true`s would leave the session stuck at the divisor it had.
  strains = [];
  run(T, { fps: 88, ping: 30, dec: 12.0, jitter: 4, kbps: 7800, lossPct: 0.0, decodeDropPct: 6.0,
           availableKbps: 20000 });
  want("unlocked first: the shed is asked for", strains, [true]);
  W.fpsLock = true;
  W.health({ fps: 88, ping: 30, jbuf: null, dec: 12.0, jitter: 4, kbps: 7800, lossPct: 0.0,
             decodeDropPct: 6.0, availableKbps: 20000, targetFps: T.fps });
  want("locking mid-shed releases the latch", strains, [true, false]);
  W.fpsLock = false;

  // The regression that a constant-fed test cannot catch, and the one that actually shipped.
  //
  // Observed 2026-09-12 21:43: a decode time hovering around its budget flipped the flag about
  // every 1.5 s, and the compositor walked 1 -> 2 -> 1 -> 2 for a minute. The cause was reaching
  // past the settled verdict to this tick's reading. A stream whose samples straddle the budget
  // must produce ONE report, not one per crossing — the settle window is what decides, and it is
  // already applied by the time strain is read.
  strains = [];
  W.setTargetKbps(T.kbps);
  // `fps: 60` against a target of 90 is what makes this decoder genuinely behind. It used to be
  // 88, which is a decoder keeping up — and since `keepingUp` landed, keeping up is no longer
  // saturation however long each frame takes, so the old snapshot correctly reports nothing and
  // could not exercise the flap at all. The property under test is unchanged: a settled verdict
  // must not flip because this tick's reading crossed a line.
  const near = (dec) => ({ fps: 60, ping: 30, dec, jitter: 4, kbps: 7800, lossPct: 0.0,
                           decodeDropPct: 0.0, availableKbps: 20000, targetFps: T.fps });
  // Budget at 90 fps is 11.1 ms, so the device rule fires at 10.0 ms. The run below settles the
  // verdict to "your device" and then dips under that line every few ticks — which is what a
  // decoder working near its limit actually looks like, and is not the same as a verdict that
  // changes. A perfectly alternating signal never settles at all and correctly reports nothing;
  // this is the case that *does* settle and used to flap underneath the settled answer.
  for (let i = 0; i < WARMUP + 3; i++) W.health(near(12.4));   // settle to bad/your device
  for (const dec of [9.6, 12.8, 13.1, 9.8, 12.0, 12.4, 9.5, 12.9, 11.2, 9.9, 12.2, 12.6,
                     9.7, 13.0, 12.5, 10.6, 9.4, 12.9, 11.5, 12.1]) {
    W.health(near(dec));
  }
  want("a decode time dipping under its budget does not flap the flag", strains, [true]);

  // ── Latency is not saturation ────────────────────────────────────────────
  //
  // The live false positive of 2026-09-13 12:23:03: `bad your device, decode 16.4 ms against a
  // 11.1 ms budget` reported strain and shed the compositor to 1-in-2, while the same snapshot
  // said `fps=90` of 90 with `framesDropped` flat. A pipelined hardware decoder holds per-frame
  // latency above the frame interval and still sustains full throughput; reading that as
  // saturation halves the frame rate of a phone that is not behind by a single frame.
  //
  // Ordering note: the healthy-ending case goes **last**. Every scenario logs an `ok healthy`
  // line when `setTargetKbps` resets the verdict, so two scenarios in a row that end healthy
  // leave two identical adjacent lines and trip the repeat check below for no real reason.

  // The case the whole mechanism exists for still fires: a phone decoding a fraction of what it
  // is sent fails `keepingUp` on throughput, whatever its per-frame time says.
  strains = [];
  W.setTargetKbps(T.kbps);
  run(T, { fps: 15, ping: 27, dec: 16.4, jitter: 3, kbps: 7800, lossPct: 0.0,
           decodeDropPct: 0.0, availableKbps: 20000 });
  want("a decoder that is actually behind still reports strain", strains, [true]);

  // Frames dropped after arriving are the other half of the evidence — a decoder can hold the
  // frame rate by discarding, and that is still saturation. A different decode figure from the
  // case above so the two verdict lines are distinguishable in the log.
  strains = [];
  W.setTargetKbps(T.kbps);
  run(T, { fps: 90, ping: 27, dec: 22.0, jitter: 3, kbps: 7800, lossPct: 0.0,
           decodeDropPct: 12.0, availableKbps: 20000 });
  want("a decoder holding its rate by dropping still reports strain", strains, [true]);

  // And the false positive itself: keeping up, so not saturated, so no strain and no shed.
  strains = [];
  W.setTargetKbps(T.kbps);
  run(T, { fps: 90, ping: 27, dec: 16.4, jitter: 3, kbps: 7800, lossPct: 0.0,
           decodeDropPct: 0.0, availableKbps: 20000 });
  want("a pipelined decoder keeping up reports no strain", strains, []);

  // And it clears: otherwise the daemon sheds for the rest of the session on one bad minute.
  strains = [];
  run(T, { fps: 88, ping: 30, dec: 12.0, jitter: 4, kbps: 7800, lossPct: 0.0, decodeDropPct: 6.0,
           availableKbps: 20000 });
  const before = strains.length;
  for (let i = 0; i < 6; i++) {
    W.health({ fps: 90, ping: 27, dec: 3.0, jitter: 3, kbps: 7800, lossPct: 0.0,
               decodeDropPct: 0.0, availableKbps: 20000, targetFps: T.fps, jitterBufferTarget: null });
  }
  want("strain clears when the phone recovers", strains.slice(before), [false]);
}

// ── A shed the phone asked for is not the server failing ─────────────────────────────────────
//
// Measured 2026-09-12 16:17:33, one minute after the backoff shipped: the strip read
// `bad the server — only 816 kbps arriving of 5.7 Mbps` about a frame rate this phone had
// requested three seconds earlier. Shedding legitimately cuts both the bitrate and the frame
// rate, so every term in the starvation rule reads as a dead sender unless it scales.
{
  const shed = (divisor, snapshot, wantSide, wantState) => {
    W.setTargetKbps(T.kbps);
    W.setShedding(divisor);
    const s = { fps: null, ping: null, jbuf: null, dec: null, jitter: null, kbps: null,
                lossPct: null, decodeDropPct: null, availableKbps: null,
                targetFps: T.fps, ...snapshot };
    for (let i = 0; i < 9; i++) W.health(s);
    const ok = out.side === wantSide && out.state === wantState;
    if (!ok) { failures++; console.log(`FAIL shed 1-in-${divisor}: got ${out.state}/${out.side} "${out.detail}", want ${wantState}/${wantSide}`); }
    else console.log(`ok   shed 1-in-${divisor}  →  ${out.state}/${out.side}  ${out.detail}`);
  };

  // 1 tick in 4 of an 8 Mbps / 90 fps session: ~2 Mbps and ~22 fps is exactly right, and the
  // decode budget is 44 ms, not 11 — the phone has four times as long per frame.
  shed(4, { fps: 22, ping: 30, dec: 20.0, jitter: 4, kbps: 2000, lossPct: 0.0,
            decodeDropPct: 0.0, availableKbps: 20000 },
       "healthy", "ok");

  // The same shed, but nothing is actually arriving: a dead sender must still be caught
  // underneath an active mitigation, or shedding becomes a blindfold.
  shed(4, { fps: 3, ping: 30, dec: 3.0, jitter: 3, kbps: 90, lossPct: 0.0,
            decodeDropPct: 0, availableKbps: 20000 },
       "the server", "bad");

  // And the phone can still be over its budget at the reduced rate — 50 ms against 44 ms.
  shed(4, { fps: 21, ping: 30, dec: 50.0, jitter: 4, kbps: 2000, lossPct: 0.0,
            decodeDropPct: 3.0, availableKbps: 20000 },
       "your device", "bad");

  W.setShedding(1);   // leave the module as the next case expects it
}

// The relay is "on change only", and the invariant that actually matters is that no two
// consecutive lines are the same — a count is brittle, because a session legitimately logs a
// settled verdict after the reset each case performs.
//
// **Within a session.** `setTargetKbps` clears `lastVerdict`, so two *different* sessions that
// both settle on the same wording are not a repetition bug — and two that both end healthy are
// the common case. The wrapper below drops a marker into the log at every reset so a comparison
// can never straddle one; without it, adding any scenario that ends healthy trips this check.
const dup = logged.findIndex((l, i) => i > 0 && l === logged[i - 1] && l !== SESSION_MARK);
if (dup > 0) { failures++; console.log(`FAIL rlog: line ${dup} repeats the one before it`); }
else console.log(`ok   relayed ${logged.length} verdict lines, no consecutive repeats`);

// ── The discriminator: what was sent, versus what arrived ────────────────────
//
// Three faults with identical symptoms on this side — little arriving, `packetsLost = 0`, frame
// rate down. Before `sent_kbps` the rule called all three "the server", and it was wrong about
// it live three times (22:33, 01:19, 12:04). These cases are the ones that collapse if the
// three-way split is ever flattened back into one.
const STARVED = { fps: 20, ping: 30, dec: 4.0, jitter: 3, kbps: 500,
                  lossPct: 0.0, decodeDropPct: 0, availableKbps: 20000 };

// 1. The daemon sent nearly everything it was asked for, and a twentieth of it arrived, with the
//    loss counter silent. The bytes went into the path and did not come out.
check("sent but not arrived → the path, not the sender", T, STARVED, "network", "bad", 7600);

// 2. The daemon says itself that it is barely sending. Now the accusation has evidence.
check("not sent at all → the server, with its own number", T, STARVED, "the server", "bad", 300);
if (!/sent only/.test(out.detail)) {
  failures++; console.log(`FAIL the server's own number should be quoted, got "${out.detail}"`);
} else { console.log("ok   …quoting the daemon's own figure, not an inference"); }

// 3. No report — an older daemon, or the first stretch of a session. The verdict still lands on
//    the server, but it must say the number is missing rather than sound as certain as (2).
check("no report → the server, hedged", T, STARVED, "the server", "bad", null);
if (!/has not reported/.test(out.detail)) {
  failures++;
  console.log(`FAIL an absent sent-bitrate must be admitted, got "${out.detail}"`);
} else {
  console.log("ok   …and says the number is missing rather than sounding certain");
}

// 4. An absent report must not read as zero. Zero would make (3) accuse the server *harder*
//    than (2) does, on no evidence at all.
check("an absent report is not zero", T, STARVED, "the server", "bad", NaN);
if (/sent only/.test(out.detail)) {
  failures++;
  console.log(`FAIL an absent report was treated as zero: "${out.detail}"`);
} else {
  console.log("ok   …and is not treated as a measured zero");
}

// 5. Under a shed, the comparison scales — a shed session legitimately sends a fraction, and the
//    sent figure has to be judged against the *reduced* expectation, not the original.
// A shed session legitimately sends a fraction of the original target, so the sent figure has to
// be judged against the *reduced* expectation. `setShedding` comes after `check`'s reset, so it
// is applied through the snapshot loop instead — see the divisor note in health.js.
W.setTargetKbps(T.kbps);
W.setSentKbps(1900);       // ~= 8000/4: exactly what a 1-in-4 shed should be putting out
W.setShedding(4);
{
  const s2 = { fps: null, ping: null, jbuf: null, dec: null, jitter: null,
               targetFps: T.fps, ...STARVED, fps: 5, kbps: 120 };
  for (let i = 0; i < 9; i++) W.health(s2);
  const ok = out.side === "network" && out.state === "bad";
  if (!ok) { failures++; console.log(`FAIL under a shed, a correct sender is accused: got ${out.state}/${out.side} "${out.detail}"`); }
  else console.log(`ok   under a shed, a correct sender is not accused  →  ${out.state}/${out.side}  ${out.detail}`);
}
W.setShedding(1);

// ── A hidden page has nothing useful to say ─────────────────────────────────
//
// Measured live 2026-09-13 12:34:57 with the page hidden: `bad network — only 65 kbps arriving
// of 11.9 Mbps the server actually sent`. True as stated, and completely the wrong conclusion —
// the bytes reached the browser and the browser discarded them, because nobody was watching.
{
  globalThis.document = { visibilityState: "visible", hasFocus: () => true };

  // First make it strain while visible, so there is a flag to withdraw.
  strains = [];
  const behind = { fps: 15, ping: 27, dec: 40.0, jitter: 3, kbps: 7800, lossPct: 0.0,
                   decodeDropPct: 0.0, availableKbps: 20000 };
  check("a strained visible page reports it", T, behind, "your device", "bad");
  if (strains[strains.length - 1] !== true) {
    failures++; console.log(`FAIL expected a strain report while visible, got ${JSON.stringify(strains)}`);
  }

  // Now hide it. The throttled numbers must not produce a verdict at all.
  globalThis.document.visibilityState = "hidden";
  strains = [];
  const hidden = { fps: 73, ping: 35, dec: 12.3, jitter: 4, kbps: 65, lossPct: 0.0,
                   decodeDropPct: 0.0, availableKbps: 55, targetFps: T.fps };
  for (let i = 0; i < 9; i++) W.health(hidden);
  if (!(out.state === "ok" && out.side === "not watching")) {
    failures++;
    console.log(`FAIL a hidden page must not produce a verdict: got ${out.state}/${out.side} "${out.detail}"`);
  } else {
    console.log("ok   a hidden page reports 'not watching', not a fault");
  }
  if (strains[strains.length - 1] !== false) {
    failures++;
    console.log(`FAIL a hidden page must withdraw the strain flag, got ${JSON.stringify(strains)}`);
  } else {
    console.log("ok   …and withdraws the strain flag so the compositor stops shedding");
  }
  delete globalThis.document;
}

console.log(failures ? `\n${failures} FAILED` : "\nall passed");
process.exit(failures ? 1 : 0);
