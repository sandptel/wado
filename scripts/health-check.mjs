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
W.rlog = (l) => logged.push(l);   // the bridge supplies this; here it is captured
const emit = (m) => { out = m; };
new Function("W", "emit", src)(W, emit);

let failures = 0;
const check = (name, target, snapshot, wantSide, wantState) => {
  W.setTargetKbps(target.kbps);        // also resets the warm-up
  const s = { fps: null, ping: null, jbuf: null, dec: null, jitter: null, kbps: null,
              lossPct: null, decodeDropPct: null, availableKbps: null,
              targetFps: target.fps, ...snapshot };
  for (let i = 0; i < 6; i++) W.health(s);   // burn the warm-up, then the real verdict
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
check("link too small, and it shows", T,
  { fps: 60, ping: 30, dec: 4.0, jitter: 4, kbps: 1300, lossPct: 0.1, decodeDropPct: 0, availableKbps: 1400 },
  "network", "bad");

// The same shortfall with nothing suffering is NOT a fault. Chrome's availableIncomingBitrate
// tracks the received rate while uncongested, so a static screen sending 600 kbps reports a
// 600 kbps "link" — and the old rule called that a broken network once a second.
check("low link estimate with a healthy stream is not a fault", T,
  { fps: 90, ping: 30, dec: 4.0, jitter: 4, kbps: 700, lossPct: 0.0, decodeDropPct: 0, availableKbps: 750 },
  "healthy", "ok");

// A still screen encodes to almost nothing. Throughput alone must never accuse the server.
check("idle screen is not a fault", T,
  { fps: 90, ping: 27, dec: 3.0, jitter: 3, kbps: 60, lossPct: 0.0, decodeDropPct: 0, availableKbps: 20000 },
  "healthy", "ok");

// The verdict is also relayed to the daemon log, on change only — that is what makes a
// session diagnosable afterwards. Six distinct verdicts above, so six lines and no repeats.
if (logged.length !== 6) { failures++; console.log(`FAIL rlog: ${logged.length} lines, want 6`); }
else console.log(`ok   relayed ${logged.length} verdict lines, e.g. ${JSON.stringify(logged[1])}`);

console.log(failures ? `\n${failures} FAILED` : "\nall passed");
process.exit(failures ? 1 : 0);
