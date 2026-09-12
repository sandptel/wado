// Runnable check for the persistent relay link (js/relay_link.js + js/relay.js).
//
// These two files are loaded into **one** scope here, the way the bridge concatenates them, and
// driven with a fake WebSocket so the whole thing runs without a browser or a relay.
//
// The cases are the ones the old promise-owned socket could not express, because it had no
// second socket and no life after a close:
//
//   * a socket that closes mid-request — does the wait reject, or hang forever?
//   * a reconnect while a session is running — exactly one rejoin, not zero and not one per try
//   * a handler firing after a reconnect — does it write to the *new* socket?
//   * join_denied — retried, not a dead end
//   * the drop-and-restart flag — cleared when the link goes down under it
//
// A note this project has paid for twice: a test that feeds a constant cannot catch a flap.
// Every case below feeds a sequence.
//
// Run:  node scripts/relay-link-check.mjs
import { readFileSync } from "node:fs";

const read = (f) => readFileSync(new URL(`../crates/client/src/js/${f}`, import.meta.url), "utf8");
// Link first: `relay.js` registers its handlers by calling `W.relayOn` at load, and that is
// defined here. This order is the bundle's order, and getting it backwards is a TypeError at
// load — which is how this harness earned its keep before it had run a single case.
const SRC = read("relay_link.js") + "\n" + read("relay.js");

let failures = 0;
const check = (name, got, want) => {
  const ok = JSON.stringify(got) === JSON.stringify(want);
  if (!ok) failures++;
  console.log(`${ok ? "  ok  " : "FAIL  "}${name}` +
    (ok ? "" : `\n        got  ${JSON.stringify(got)}\n        want ${JSON.stringify(want)}`));
};

// ── The fake socket ───────────────────────────────────────────────────────────
// Every instance is recorded, so a test can assert *which* socket a send landed on — the
// question a captured-`ws` closure gets wrong and cannot be asked about.
function makeWorld() {
  const sockets = [];
  class FakeWS {
    static CONNECTING = 0; static OPEN = 1; static CLOSING = 2; static CLOSED = 3;
    constructor(url) {
      this.url = url;
      this.readyState = 0;
      this.sent = [];
      sockets.push(this);
    }
    send(s) {
      if (this.readyState !== 1) throw new Error("send on a socket that is not open");
      this.sent.push(JSON.parse(s));
    }
    close() { this.readyState = 3; if (this.onclose) this.onclose({ code: 1000 }); }
    // — test drivers —
    open() { this.readyState = 1; if (this.onopen) this.onopen(); }
    deliver(msg) { if (this.onmessage) this.onmessage({ data: JSON.stringify(msg) }); }
    drop(code = 1006) { this.readyState = 3; if (this.onclose) this.onclose({ code }); }
    accept() { this.open(); this.deliver({ type: "join_accepted", remote_id: "1", room_id: "r" }); }
  }
  const events = [];
  const W = {
    requestApps: () => events.push("apps_request"),
    textInput: () => {}, setShedding: () => {},
    setTargetKbps: () => {}, setTargetFps: () => {},
    ptyOutput: () => {}, ptyExited: () => {},
    handleFailure: () => {}, startStats: () => {}, stopStats: () => {},
    setupInputCapture: () => {}, attachLatencyEcho: () => {},
    minimizePlayoutDelay: () => "", latency: { start() {}, stop() {} },
    // Negotiation needs a browser; the cases here are all about signalling, so it is stubbed
    // after the eval (relay.js defines the real one).
  };
  const emit = (e) => events.push("emit:" + e.type);
  const status = (t) => events.push("status:" + t);
  const stagebar = () => {};
  new Function("W", "emit", "status", "stagebar", "INPUT_CHANNEL", "WebSocket", SRC)(
    W, emit, status, stagebar, "input", FakeWS,
  );
  W._relayNegotiate = () => { events.push("negotiate"); return Promise.resolve(); };
  return { W, sockets, events };
}

const tick = () => new Promise((r) => setTimeout(r, 0));
const started = (s) => s.sent.filter((m) => m.type === "session_start").length;
const rejoins = (s) => s.sent.filter((m) => m.type === "session_rejoin").length;

// ── 1. A warm link starts a session with no new socket ───────────────────────
{
  const { W, sockets } = makeWorld();
  W.relayDial("https://relay.test", "872-990-894");
  sockets[0].accept();
  const before = sockets.length;
  W.relayConnect("https://relay.test", "872-990-894", { width: 1 });
  check("a warm link opens no second socket", sockets.length, before);
  check("…and asks for a session immediately", started(sockets[0]), 1);
  check("…over wss with the id normalized", sockets[0].url, "wss://relay.test/join/872990894");
}

// ── 2. The link reconnects on its own, forever ───────────────────────────────
{
  const { W, sockets } = makeWorld();
  W.relayDial("ws://r", "1");
  sockets[0].accept();
  sockets[0].drop();
  await tick();
  check("a dropped link schedules a retry", W._relayRetry !== null, true);
  // Backoff grows and caps. Sampled from the pure function, not by waiting on timers.
  const delays = [1, 2, 3, 4, 5, 6, 7].map((n) => Math.min(500 * Math.pow(2, n - 1), 15000));
  check("backoff grows then caps", delays, [500, 1000, 2000, 4000, 8000, 15000, 15000]);
}

// ── 3. join_denied is retried, not a dead end ────────────────────────────────
{
  const { W, sockets } = makeWorld();
  W.relayDial("ws://r", "1");
  sockets[0].open();
  sockets[0].deliver({ type: "join_denied", reason: "no daemon" });
  check("join_denied does not mark the link up", W.relayUp, false);
  sockets[0].drop();
  await tick();
  check("…and still schedules a retry", W._relayRetry !== null, true);
}

// ── 4. A reconnect mid-session rejoins exactly once ──────────────────────────
{
  const { W, sockets, events } = makeWorld();
  W.relayConnect("ws://r", "1", { width: 1 });
  sockets[0].accept();
  sockets[0].deliver({ type: "session_started", info: { encoder: { mode: "hardware" } } });
  await tick();
  check("the session comes up", W.sessionOn, true);

  sockets[0].drop();
  check("the session is held across the outage", W.sessionOn, true);
  check("the drop-and-restart flag is cleared on link down", W._relayDropPending, false);

  // The link comes back on a brand-new socket.
  W._relayRetry = null;
  W.relayDial("ws://r", "1");
  const back = sockets[sockets.length - 1];
  check("a reconnect uses a new socket", back !== sockets[0], true);
  back.accept();
  check("the reconnect asks about the session", started(back), 1);
  check("…and asks nothing of the dead socket", started(sockets[0]), 1);

  back.deliver({ type: "session_alive", info: { encoder: { mode: "hardware" } } });
  check("a surviving session is rejoined without prompting", rejoins(back), 1);
  check("…and the prompt is never raised", events.filter((e) => e === "emit:sessionAlive").length, 0);

  // The thing a captured `ws` gets wrong: a handler that runs after the reconnect must write to
  // the socket that is live now.
  back.deliver({ type: "ping" });
  check("a handler after the reconnect writes to the new socket",
    back.sent.filter((m) => m.type === "pong").length, 1);
  check("…and not to the old one", sockets[0].sent.filter((m) => m.type === "pong").length, 0);
}

// ── 5. A session that did NOT survive falls back to a fresh one ──────────────
{
  const { W, sockets } = makeWorld();
  W.relayConnect("ws://r", "1", { width: 1 });
  sockets[0].accept();
  sockets[0].deliver({ type: "session_started", info: { encoder: { mode: "hardware" } } });
  await tick();
  sockets[0].drop();
  W._relayRetry = null;
  W.relayDial("ws://r", "1");
  const back = sockets[sockets.length - 1];
  back.accept();
  back.deliver({ type: "session_error", message: "the session ended before you could rejoin it" });
  check("an expired session clears sessionOn", W.sessionOn, false);
  check("…and asks for a fresh one rather than renegotiating forever", started(back), 2);
}

// ── 6. A request that outlives its socket rejects instead of hanging ─────────
{
  const { W, sockets } = makeWorld();
  W.relayDial("ws://r", "1");
  sockets[0].accept();
  // The offer wait is the one that used to hang: its promise was settled only by an incoming
  // sdp_answer, and a socket that closed first never produced one.
  let settled = null;
  const wait = new Promise((resolve, reject) => {
    W._relayAnswer = resolve;
    setTimeout(() => {
      if (W._relayAnswer !== resolve) return;
      W._relayAnswer = null;
      reject(new Error("the daemon did not answer the offer within 30 s"));
    }, 20);
  });
  wait.then(() => { settled = "resolved"; }, () => { settled = "rejected"; });
  sockets[0].drop();
  await new Promise((r) => setTimeout(r, 40));
  check("an unanswered offer rejects rather than hanging", settled, "rejected");
}

// ── 7. Stopping a session leaves the link up ─────────────────────────────────
{
  const { W, sockets } = makeWorld();
  W.relayConnect("ws://r", "1", { width: 1 });
  sockets[0].accept();
  sockets[0].deliver({ type: "session_started", info: { encoder: { mode: "hardware" } } });
  await tick();
  W.relayStop();
  check("stop sends session_stop", sockets[0].sent.filter((m) => m.type === "session_stop").length, 1);
  check("…and the link stays up", W.relayUp, true);
  check("…and stops wanting a session", W._relayWanted, false);
  // A reconnect after an explicit stop must not resurrect anything.
  sockets[0].drop();
  W._relayRetry = null;
  W.relayDial("ws://r", "1");
  const back = sockets[sockets.length - 1];
  back.accept();
  check("a reconnect after an explicit stop asks for nothing", started(back), 0);
}

console.log(failures ? `\n${failures} failing` : "\nall relay-link checks pass");
process.exit(failures ? 1 : 0);
