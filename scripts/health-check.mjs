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

let failures = 0;
const check = (name, target, snapshot, wantSide, wantState) => {
  W.setTargetKbps(target.kbps);        // also resets the warm-up
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

  // The regression that a constant-fed test cannot catch, and the one that actually shipped.
  //
  // Observed 2026-09-12 21:43: a decode time hovering around its budget flipped the flag about
  // every 1.5 s, and the compositor walked 1 -> 2 -> 1 -> 2 for a minute. The cause was reaching
  // past the settled verdict to this tick's reading. A stream whose samples straddle the budget
  // must produce ONE report, not one per crossing — the settle window is what decides, and it is
  // already applied by the time strain is read.
  strains = [];
  W.setTargetKbps(T.kbps);
  const near = (dec) => ({ fps: 88, ping: 30, dec, jitter: 4, kbps: 7800, lossPct: 0.0,
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

// The relay is "on change only", and the invariant that actually matters is that no two
// consecutive lines are the same — a count is brittle, because a session legitimately logs a
// settled verdict after the reset each case performs.
const dup = logged.findIndex((l, i) => i > 0 && l === logged[i - 1]);
if (dup > 0) { failures++; console.log(`FAIL rlog: line ${dup} repeats the one before it`); }
else console.log(`ok   relayed ${logged.length} verdict lines, no consecutive repeats`);

console.log(failures ? `\n${failures} FAILED` : "\nall passed");
process.exit(failures ? 1 : 0);
