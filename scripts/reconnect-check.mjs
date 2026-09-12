// Runnable check for WebRTC recovery routing in js/webrtc.js.
//
// The branch worth pinning: relay mode must re-offer through the relay WebSocket, never through
// connectWebRTC. connectWebRTC POSTs to `W.server + "/offer"`, which does not exist in relay
// mode — so routing a relay-mode reconnect there burns the whole retry budget on a fetch that
// cannot succeed and then reports "giving up". That was the live behaviour until 2026-09-12.
//
// Run:  node scripts/reconnect-check.mjs
import { readFileSync } from "node:fs";

const src = readFileSync(new URL("../crates/client/src/js/webrtc.js", import.meta.url), "utf8");

// `webrtc.js` is one of several files concatenated into a single eval; it reads `W`, `status`,
// `emit` and `stagebar` from that shared scope. Supplying them here is what the concatenation
// does at runtime.
function load(W) {
  const calls = [];
  W.stopStats = () => {};
  W.startStats = () => {};
  W.setupInputCapture = () => {};
  W.attachLatencyEcho = () => {};
  W.latency = { start() {}, stop() {} };
  W.minimizePlayoutDelay = () => "";
  const status = (s) => calls.push("status:" + s);
  const emit = (e) => calls.push("emit:" + e.type);
  const stagebar = () => {};
  // `RTCPeerConnection` and the channel names are only touched by connectWebRTC, which is
  // stubbed above — the eval itself only needs them to not be referenced at definition time.
  const INPUT_CHANNEL = "input", MOTION_CHANNEL = "motion";
  new Function("W", "status", "emit", "stagebar", "INPUT_CHANNEL", "MOTION_CHANNEL", src)(
    W, status, emit, stagebar, INPUT_CHANNEL, MOTION_CHANNEL,
  );
  // After the eval, not before: webrtc.js defines `W.connectWebRTC` itself and would overwrite
  // a stub installed first. `reconnectWebRTC` reads both off `W` at call time.
  W.connectWebRTC = () => { calls.push("direct"); return Promise.resolve(); };
  W._relayNegotiate = () => { calls.push("relay"); return Promise.resolve(); };
  return calls;
}

const OPEN = 1;
let failures = 0;
const check = (name, got, want) => {
  const ok = JSON.stringify(got) === JSON.stringify(want);
  if (!ok) failures++;
  console.log(`${ok ? "  ok  " : "FAIL  "}${name}${ok ? "" : `\n        got  ${JSON.stringify(got)}\n        want ${JSON.stringify(want)}`}`);
};

// 1. Relay mode, socket up — the relay path, and only the relay path.
{
  const W = { relayMode: true, sessionOn: true, relayUp: true };
  const calls = load(W);
  await W.reconnectWebRTC();
  check("relay mode re-offers through the relay link", calls, ["relay"]);
}

// 2. Direct mode — unchanged.
{
  const W = { relayMode: false, sessionOn: true, server: "http://x" };
  const calls = load(W);
  await W.reconnectWebRTC();
  check("direct mode still re-offers over HTTP", calls, ["direct"]);
}

// 3. Relay mode with the link down — reject rather than silently take the wrong path. The link
//    reconnects on its own (see relay_link.js), so rejecting costs one retry, not the session.
{
  const W = { relayMode: true, sessionOn: true, relayUp: false };
  const calls = load(W);
  let rejected = false;
  await W.reconnectWebRTC().catch(() => { rejected = true; });
  check("a down relay link rejects instead of falling back to HTTP", [rejected, ...calls], [true]);
}

// 4. handleFailure routes through the same chooser, on a timer.
{
  const W = { relayMode: true, sessionOn: true, relayUp: true, reconnectAttempts: 0 };
  const calls = load(W);
  W.handleFailure();
  check("handleFailure counts the attempt before waiting", W.reconnectAttempts, 1);
  await new Promise((r) => setTimeout(r, 700));
  check("handleFailure re-offers via relay", calls.filter((c) => c === "relay" || c === "direct"), ["relay"]);
}

// 5. resync — the manual lever against the jitter-buffer ratchet — must not use the HTTP path.
{
  const W = { relayMode: true, sessionOn: true, relayUp: true, reconnectAttempts: 4 };
  const calls = load(W);
  await W.resync();
  check("resync re-offers via relay and clears the attempt count",
    [calls.filter((c) => c === "relay" || c === "direct"), W.reconnectAttempts], [["relay"], 0]);
}

// 6. The retry budget must fit inside the server's VIEWER_GRACE, or a reclaimable session is
//    abandoned while the daemon is still holding it. Both numbers are read from the source, so
//    moving one and not the other fails here rather than in the field — which is exactly what
//    happened when the grace went 45 s -> 600 s and the budget stayed at ~37 s.
{
  // MAX_RECONNECTS lives in core.js, which is concatenated ahead of webrtc.js.
  const core = readFileSync(new URL("../crates/client/src/js/core.js", import.meta.url), "utf8");
  const max = Number(/W\.MAX_RECONNECTS\s*=\s*(\d+)/.exec(core)[1]);
  const rs = readFileSync(new URL("../crates/server/src/relay_client.rs", import.meta.url), "utf8");
  const graceMs = Number(/VIEWER_GRACE:\s*std::time::Duration\s*=\s*std::time::Duration::from_secs\((\d+)\)/.exec(rs)[1]) * 1000;
  let total = 0;
  for (let n = 1; n <= max; n++) total += Math.min(500 * Math.pow(2, n - 1), 5000);
  check("the retry budget fits inside the server grace", total < graceMs, true);
  // And is not so short that an ordinary dead zone outlasts it.
  check("the retry budget outlasts a two-minute dead zone", total >= 120000, true);
}

console.log(failures ? `\n${failures} failed` : "\nall checks passed");
process.exit(failures ? 1 : 0);
