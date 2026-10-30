// wado bridge — how long the computer took to answer, on this device's own clock.
//
// The daemon times its own stages (`server::slow`, `slow:` lines); this is the other end: from a
// request leaving to its answer arriving, one clock, no sync. A slow answer is said through
// W.rlog as `ask: <request> → <answer> in N ms`; a request still unanswered after STILL_MS is
// said once as `ask: <request> still unanswered after N ms` — the moment a person starts to
// wonder — so the daemon log shows the wait from both ends by kind.
//
//   W.askSent(type)  — called by W.relaySendMsg for every request that went out
//   W.askGot(type)   — called for every frame that arrived
//
// Only the pairs below are timed; anything else costs one object lookup.

const ASK_SLOW_MS = 1000;
const ASK_STILL_MS = 5000;

// request type → the replies that answer it (any one of them).
const ASK_PAIRS = {
  session_start: ["session_started", "session_alive", "session_error"],
  session_rejoin: ["session_started", "session_error"],
  session_join: ["session_started", "session_error", "session_left"],
  session_takeover: ["session_started", "session_error"],
  session_reconfigure: ["session_reconfigured", "session_error"],
  session_stop: ["session_stopped", "session_error"],
  session_launch: ["session_launched", "session_error"],
  session_window: ["session_windowed", "session_error"],
  sdp_offer: ["sdp_answer", "session_error"],
  apps_request: ["apps_list"],
  sessions_request: ["sessions"],
  shells_request: ["shells"],
  pty_open: ["pty_opened", "session_error"],
  host_get: ["host_state", "host_error"],
  host_do: ["host_state", "host_error"],
  config_get: ["config_state"],
  timing_request: ["timing"],
  desk_request: ["session_elsewhere", "desks_full", "session_error"],
  link_test: ["link_test_done"],
  link_report: ["link_verdict"],
  files_offer: ["files_answer"],
  pad_offer: ["pad_answer"],
  clip_get: ["clip_data", "clip_error"],
  passkey_begin: ["passkey_challenge", "passkey_state"],
  passkey_unlock: ["passkey_state"],
  passkey_register: ["passkey_state"],
  host_net_request: ["host_net"],
};

// reply type → the requests it may answer.
const ASK_BY_REPLY = {};
for (const [req, replies] of Object.entries(ASK_PAIRS)) {
  for (const r of replies) (ASK_BY_REPLY[r] = ASK_BY_REPLY[r] || []).push(req);
}

const askOpen = new Map(); // request type → {t0, told}  (the oldest unanswered one)

W.askSent = (type) => {
  if (!ASK_PAIRS[type] || askOpen.has(type)) return;
  askOpen.set(type, { t0: performance.now(), told: false });
};

W.askGot = (type) => {
  const reqs = ASK_BY_REPLY[type];
  if (!reqs) return;
  const now = performance.now();
  for (const req of reqs) {
    const a = askOpen.get(req);
    if (!a) continue;
    askOpen.delete(req);
    const ms = Math.round(now - a.t0);
    if ((ms >= ASK_SLOW_MS || a.told) && W.rlog) W.rlog("ask: " + req + " → " + type + " in " + ms + " ms");
    return; // one reply answers one request
  }
};

// The link went down: nothing in flight will be answered on it. Said, so a give-up is not silent.
W.askDropped = () => {
  const now = performance.now();
  const lost = [];
  for (const [req, a] of askOpen) lost.push(req + " (" + Math.round(now - a.t0) + " ms)");
  askOpen.clear();
  if (lost.length && W.rlog) W.rlog("ask: link dropped with no answer to " + lost.join(", "));
};

setInterval(() => {
  const now = performance.now();
  for (const [req, a] of askOpen) {
    if (!a.told && now - a.t0 >= ASK_STILL_MS) {
      a.told = true;
      if (W.rlog) W.rlog("ask: " + req + " still unanswered after " + Math.round(now - a.t0) + " ms");
    }
  }
}, 1000);
