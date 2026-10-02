// wado bridge — the computer's running sessions, on any daemon of its pool (server::sessions),
// and the two ways back to one from the home page: Resume and End. Also Leave: step out to the
// home page and keep the session running.
//
// A session on another daemon is reached by re-dialling the relay for *that* daemon (the join's
// `instance`), taking its seat if a device holds it, then doing the action once the link is up.

W.sessionsGet = () => W.relayUp && W.relaySendMsg({ type: "sessions_request" });
W.relayOn("sessions", (msg) =>
  emit({ type: "sessions", sessions: msg.sessions || [], here: (W.pool && W.pool.instance) || "" }));

let sessionsTimer = 0;
W.sessionsWatch = (on) => {
  clearInterval(sessionsTimer);
  if (on) { W.sessionsGet(); sessionsTimer = setInterval(W.sessionsGet, 5000); }
};

// Leave: the session keeps running (the daemon is told not to reap it); this page goes home.
W.leaveSession = () => {
  W.relaySendMsg({ type: "session_detach" });
  try { localStorage.removeItem("wado.watching"); } catch (_) {}
  W._relayWanted = false;
  W._relayResuming = false;
  W._listenOnly = false;
  if (W.pc) { try { W.pc.close(); } catch (_) {} W.pc = null; }
  if (W.detachStream) W.detachStream();
  if (W.stopStats) W.stopStats();
  W.sessionOn = false;
  emit({ type: "sessionOff" });
  setTimeout(W.sessionsGet, 600);
};

// New session while this daemon already has one: leave its seat (freed, not held) and redial
// with no daemon asked for, so the relay hands over an idle one (it puts those first).
W.freshDaemon = () => {
  const t = W._relayTarget;
  if (!t) return;
  W.rememberInstance(String(t.id).replace(/[\s-]/g, ""), "");
  try { localStorage.removeItem("wado.watching"); } catch (_) {}
  const { url, id } = t;
  W.relayDrop(true);
  W.relayDial(url, id);
};

let afterUp = null; // "resume" | "stop", for a session on another daemon

function act(kind) {
  if (kind === "resume") {
    W._relayResuming = true;
    W.relaySendMsg({ type: "session_rejoin" });
  } else {
    W.relaySendMsg({ type: "session_stop" });
    setTimeout(W.sessionsGet, 800);
  }
}

W.sessionAct = (instance, kind) => {
  const here = W.pool && W.pool.instance;
  if (instance === here && W.relayUp) { act(kind); return; }
  const t = W._relayTarget;
  if (!t) return;
  const rid = String(t.id).replace(/[\s-]/g, "");
  W.rememberInstance(rid, instance);
  W._relayTakeover = true;   // a held seat is this person's own session: take it
  afterUp = kind;
  const { url, id } = t;
  W.relayDrop(true);
  W.relayDial(url, id);
};

{
  const up = W._relayHandlers.__up;
  W.relayOn("__up", (m) => {
    if (up) up(m);
    if (afterUp) { const k = afterUp; afterUp = null; act(k); }
    W.sessionsGet();
  });
}
