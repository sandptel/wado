// wado bridge — the relay **link**. One job: keep a WebSocket to wado-relay open, and hand
// each message to whoever registered for its type.
//
// It knows nothing about sessions, WebRTC or encoders. `relay.js` is what knows those, and it
// talks to the daemon through here.
//
// ## Why this is its own file, and its own lifetime
//
// The link used to be created *inside* one connection attempt's promise: the socket, a single
// 15 s "connection timed out" reject, the join verdict, `session_start` and the whole WebRTC
// negotiation all lived in one closure, and `ws.onclose` did nothing but null the handle. Three
// consequences, all of them things the user has hit:
//
//   * Every attempt re-dialled from nothing. A phone coming out of a lift paid for a fresh
//     WebSocket, a fresh TLS handshake through the tunnel and a fresh join before it could even
//     ask to come back — and if any of that was slow, the 15 s timer rejected the whole attempt.
//   * One timeout covered four different waits, so "handshake stalled" was the answer whether
//     the relay was down, no daemon was registered, or the encoder was merely slow to open.
//   * A dropped socket could not be retried, because the thing that would retry it was the
//     promise that had already rejected.
//
// So: the link is dialled once, as early as the relay URL is known, and it stays up. Reconnects
// are its own business, with backoff, **forever** — a page that is open is a viewer that wants
// to be connected. Pressing Start on a warm link sends one message down a socket that is
// already open, which is what the user asked for: *"why don't we stay connected to the relay
// whenever we can beforehand and just create the webrtc connection?"*
//
// Exposed:
//   W.relayDial(url, id)      idempotent — dial, keep up, reconnect. Re-targets on change.
//   W.relayDrop()             deliberate teardown; stops retrying until the next relayDial.
//   W.relayOn(type, fn)       register the handler for one message type. Two synthetic types:
//                             "__up" when the join is accepted, "__down" when the socket closes.
//   W.relaySendMsg(obj)       send if the socket is open; returns false if it was not.
//   W.relayUp                 true between join_accepted and the socket closing.
//   W.relayWs                 the live socket — kept for the callers that still reach for it.

W.relayUp = false;
W.relayWs = null;
W._relayTarget = null;      // {url, id, wsUrl}
W._relayRetry = null;       // pending reconnect timer
W._relayTries = 0;
W._relayUpAt = 0;           // when the link last reached join_accepted
W._relayDeniedOccupied = false;   // the last denial was "another viewer holds this session"
W._relayHandlers = {};      // type -> fn, plus the synthetic "__up" / "__down"

// Backoff: quick at first because most reconnects are a blip, capped low enough that a phone
// coming back from a dead zone is reconnected in seconds rather than on some slow schedule it
// happened to land in. Never gives up — see the header.
const LINK_BACKOFF_MAX = 15000;
// How long a link has to survive before its success counts. Resetting the backoff on
// `join_accepted` alone means a relay that accepts and then immediately closes — a daemon
// crash-looping, a tunnel half up — is retried every 500 ms forever, which is a hot loop
// against something already in trouble. The retries themselves stay unlimited: never giving up
// is the property that was asked for. Only the *speed* is earned.
const LINK_STABLE_MS = 5000;
// A denial because **another viewer holds the room** is not a network fault and must not be
// retried at network speed. Jumping the attempt counter straight to the cap turns a 500 ms knock
// into a 15 s one. See the `join_denied` handler for why hammering here is actively harmful.
const LINK_OCCUPIED_TRIES = 6;
// The relay's wording for that case, produced in `crates/relay/src/signaling.rs` next to
// `rooms.create`. Matched as a string because it is the only thing that distinguishes the two
// denials on the wire; `scripts/relay-link-check.mjs` reads the relay source and fails if it
// moves, so this cannot rot silently.
const OCCUPIED_RE = /already has an active connection/i;
const linkDelay = (n) => Math.min(500 * Math.pow(2, Math.max(0, n - 1)), LINK_BACKOFF_MAX);

// Which daemon of the Remote ID's pool this browser last used, keyed by Remote ID.
//
// A Remote ID names a **pool** of daemons, each with its own compositor and applications, so
// that several devices can use one ID at once. Handing the stored instance back on the next
// join is what returns this device to *its own* desktop — its windows, its running programs —
// instead of whichever daemon happened to be free. Without it, a reload or a cell handoff is
// indistinguishable from a brand-new device.
//
// Its own key, not part of `wado.settings`: this is per-browser routing state, not a setting a
// human chose, and it must not travel if settings are ever exported.
const INSTANCE_KEY = "wado.instance";

const instanceFor = (id) => {
  try { return (JSON.parse(localStorage.getItem(INSTANCE_KEY)) || {})[id] || ""; }
  catch (_) { return ""; }
};
W.rememberInstance = (id, instance) => {
  try {
    const all = JSON.parse(localStorage.getItem(INSTANCE_KEY)) || {};
    all[id] = instance;
    localStorage.setItem(INSTANCE_KEY, JSON.stringify(all));
  } catch (_) {} // private mode: stickiness is an optimisation, never a requirement
};

const toWsUrl = (relayUrl, id) => {
  const base = String(relayUrl).replace(/^https:\/\//, "wss://").replace(/^http:\/\//, "ws://");
  const norm = String(id).replace(/[\s-]/g, "");
  const want = instanceFor(norm);
  return base.replace(/\/+$/, "") + "/join/" + encodeURIComponent(norm) +
    (want ? "?instance=" + encodeURIComponent(want) : "");
};

// A uuid is unreadable in a log line and useless on a phone screen; its first block is enough
// to tell two daemons apart, which is all this is for.
W.poolTag = () => (W.pool && W.pool.instance ? W.pool.instance.slice(0, 8) : "?");

W.relayOn = (type, fn) => { W._relayHandlers[type] = fn; };

function fire(type, msg) {
  const h = W._relayHandlers[type];
  if (!h) return;
  try { h(msg); } catch (e) { if (W.rlog) W.rlog("relay handler " + type + " threw: " + e); }
}

W.relaySendMsg = (obj) => {
  const ws = W.relayWs;
  if (!ws || ws.readyState !== WebSocket.OPEN) return false;
  try { ws.send(JSON.stringify(obj)); return true; } catch (_) { return false; }
};

W.relayDial = (url, id) => {
  if (!url || !id) return false;
  const wsUrl = toWsUrl(url, id);
  // Already pointed at this relay and either connected or mid-dial: nothing to do. This is what
  // makes the call idempotent, so Start can call it unconditionally.
  if (W._relayTarget && W._relayTarget.wsUrl === wsUrl && W.relayWs &&
      (W.relayWs.readyState === WebSocket.OPEN || W.relayWs.readyState === WebSocket.CONNECTING)) {
    return true;
  }
  W.relayDrop(true);
  W._relayTarget = { url, id, wsUrl };
  W._relayTries = 0;
  W.relayMode = true;
  openLink();
  return true;
};

// `silent` is the re-target case: we are about to dial somewhere else, so this is not a
// user-visible disconnection.
W.relayDrop = (silent) => {
  if (W._relayRetry) { clearTimeout(W._relayRetry); W._relayRetry = null; }
  W._relayTarget = null;
  const ws = W.relayWs;
  W.relayWs = null;
  W.relayUp = false;
  if (ws) { try { ws.onclose = null; ws.close(); } catch (_) {} }
  if (!silent) W.relayMode = false;
};

function scheduleRelink() {
  if (!W._relayTarget || W._relayRetry) return;
  W._relayTries += 1;
  const ms = linkDelay(W._relayTries);
  if (W.rlog) W.rlog("relay link down — retry " + W._relayTries + " in " + ms + " ms");
  W._relayRetry = setTimeout(() => { W._relayRetry = null; openLink(); }, ms);
}

function openLink() {
  const target = W._relayTarget;
  if (!target) return;
  let ws;
  try {
    ws = new WebSocket(target.wsUrl);
  } catch (_) {
    scheduleRelink();
    return;
  }
  W.relayWs = ws;

  ws.onopen = () => {
    // Not "up" yet: the join verdict decides. Connecting to /join/<id> IS the join, so the
    // first message tells us whether a daemon is registered for this Remote ID.
    if (W._relayStage !== undefined) W._relayStage = 1;
    if (W.relayPhase) W.relayPhase(1, "");
  };

  ws.onmessage = (ev) => {
    let msg;
    try { msg = JSON.parse(ev.data); } catch (_) { return; }

    // Answered here rather than in a handler: the link's own liveness is the link's business,
    // and it must keep working even with no session and nothing registered.
    if (msg.type === "ping") { W.relaySendMsg({ type: "pong" }); return; }

    if (msg.type === "join_accepted") {
      W.relayUp = true;
      W._relayUpAt = Date.now();
      if (W._relayStage !== undefined) W._relayStage = 2;
      if (W.relayPhase) W.relayPhase(2, "");
      // Which daemon of the pool answered, and how full the pool is. Kept on `W` so the
      // status overlay can show it without asking the relay again, and logged on *every*
      // join — a marker that only appears when something is wrong reads the same as no
      // marker at all, and "assigned a fresh daemon" vs "came back to my own" is exactly
      // the distinction that explains a desktop full of windows or an empty one.
      W.pool = {
        instance: msg.instance_id || "",
        size: msg.pool_size || 0,
        busy: msg.pool_busy || 0,
        assignment: msg.assignment || "",
      };
      if (msg.instance_id) W.rememberInstance(String(msg.remote_id || ""), msg.instance_id);
      if (W.rlog) {
        const p = W.pool;
        W.rlog(p.size
          ? "relay link up — " +
            (p.assignment === "reclaimed" ? "back on my own daemon " : "assigned daemon ") +
            W.poolTag() + ", " + p.busy + " of " + p.size + " session(s) in use"
          : "relay link up — a daemon is registered for this Remote ID");
      }
      // `__up` reads `_relayDeniedOccupied` to decide whether getting in was our reconnect or
      // us displacing somebody, so it is cleared *after* the handler, not before.
      fire("__up");
      W._relayDeniedOccupied = false;
      return;
    }
    if (msg.type === "join_denied") {
      // Retryable, not fatal — but *how* retryable depends on which denial this is, and
      // conflating them produced a live regression on 2026-09-13.
      //
      //   "no server online"      the daemon is restarting. Retry at network speed, forever.
      //                           This is the property that was asked for.
      //   "already has an active  **another viewer holds this session.** Nothing about the
      //    connection"            network will change that, and retrying every 500 ms means
      //                           that the instant the incumbent's socket blips, we take the
      //                           room — then they knock, take it back, and the two devices
      //                           trade the session every ~18 s forever. Measured: 18 knocks
      //                           in 94 seconds, then four steal-cycles. See I17.
      const occupied = OCCUPIED_RE.test(msg.reason || "");
      if (occupied) {
        W._relayDeniedOccupied = true;
        W._relayTries = Math.max(W._relayTries, LINK_OCCUPIED_TRIES);
        // Refusal speaks as loudly as success, and carries the relay's own numbers. "The pool
        // is full" is a normal outcome once every daemon has a device, and a viewer shown a
        // bare failure cannot tell it from a broken connection — which is the whole reason
        // this branch says how many sessions exist and that another daemon raises the limit.
        W.pool = { instance: "", size: 0, busy: 0, assignment: "refused: pool full" };
        if (W.relayPhase) W.relayPhase(1, "every wado session on this Remote ID is in use");
        if (W.rlog) W.rlog("join refused — " + (msg.reason || "the pool is full"));
        return;
      }
      if (W.relayPhase) W.relayPhase(1, msg.reason || "no daemon answered for this Remote ID");
      if (W.rlog) W.rlog("join denied: " + (msg.reason || "?") + " — will keep trying");
      return;
    }

    fire(msg.type, msg);
  };

  // A socket error is always followed by a close, so there is nothing to do here that `onclose`
  // does not already do. Swallowing it stops an unhandled rejection in the console.
  ws.onerror = () => {};

  ws.onclose = (ev) => {
    // The backoff resets only for a link that actually held. See LINK_STABLE_MS.
    if (W._relayUpAt && Date.now() - W._relayUpAt >= LINK_STABLE_MS) W._relayTries = 0;
    W._relayUpAt = 0;
    if (W.relayWs === ws) { W.relayWs = null; W.relayUp = false; }
    if (W.rlog) W.rlog("relay link closed — code " + (ev && ev.code));
    fire("__down");
    scheduleRelink();
  };
}

// Dial at load if this browser already knows where to go. The settings blob is the same one the
// Rust UI writes, so a device that has connected once is connected again before anything is
// pressed — which is the point of the whole file.
try {
  const s = W.loadSettings ? W.loadSettings() : {};
  if (s && s.relay_url && s.remote_id) W.relayDial(s.relay_url, s.remote_id);
} catch (_) {}
