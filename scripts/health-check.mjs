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
const emit = (m) => { out = m; };
new Function("W", "emit", src)(W, emit);

let failures = 0;
const check = (name, target, snapshot, wantSide, wantState) => {
  W.setTargetKbps(target.kbps);
  const s = { fps: null, ping: null, jbuf: null, dec: null, jitter: null, kbps: null,
              lossPct: null, decodeDropPct: null, availableKbps: null,
              targetFps: target.fps, ...snapshot };
  W.health(s);
  const ok = out.side === wantSide && out.state === wantState;
  if (!ok) { failures++; console.log(`FAIL ${name}: got ${out.state}/${out.side} "${out.detail}", want ${wantState}/${wantSide}`); }
  else console.log(`ok   ${name}  →  ${out.state}/${out.side}  ${out.detail}`);
};

const T = { kbps: 8000, fps: 90 };          // a typical session: 8 Mbps at 90 fps

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
check("link too small", T,
  { fps: 90, ping: 30, dec: 4.0, jitter: 4, kbps: 1300, lossPct: 0.1, decodeDropPct: 0, availableKbps: 1400 },
  "network", "bad");

// A still screen encodes to almost nothing. Throughput alone must never accuse the server.
check("idle screen is not a fault", T,
  { fps: 90, ping: 27, dec: 3.0, jitter: 3, kbps: 60, lossPct: 0.0, decodeDropPct: 0, availableKbps: 20000 },
  "healthy", "ok");

console.log(failures ? `\n${failures} FAILED` : "\nall passed");
process.exit(failures ? 1 : 0);
