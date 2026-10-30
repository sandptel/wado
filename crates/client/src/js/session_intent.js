// wado bridge — the session is on screen only because the person picked it (Decision Log
// 2026-10-30, "the session never traps"). Three rules, kept here in one place:
//
//   1. Left means left. Once this device leaves (Back, Leave, End, another door), nothing that
//      was already in flight — a rejoin asked by a link blip, a re-offer from the ICE retry
//      loop — may put the session back on screen. A late `session_started` is answered with a
//      `session_detach` and goes no further. Only a pick clears it: Start, Rejoin, Join, Take
//      over, "Use it here", or the answer to the "already running" prompt.
//   2. Back leaves. Entering the session pushes a history entry, so the browser's Back, and the
//      Android app's (the WebView goes back before the app does), pops it: this device leaves
//      the session (it keeps running on the computer) and the computer's page shows
//      (`sessionBack`, `ui::surround::leave_back`).
//   3. A crash-reload takes back only a session that was showing a picture (`wado.watching`'s
//      `pic`): a session still connecting when the page died is not where the person was.

(() => {
  W._left = false;

  const pick = (name) => {
    const f = W[name];
    if (typeof f !== "function") return;
    W[name] = function (...args) { W._left = false; return f.apply(this, args); };
  };
  const leave = (name) => {
    const f = W[name];
    if (typeof f !== "function") return;
    W[name] = function (...args) { W._left = true; return f.apply(this, args); };
  };

  // After the whole bridge has loaded: `W.start` and `W.stopSession` live in lifecycle.js, the
  // last file, which never finishes evaluating.
  setTimeout(() => {
    for (const n of ["start", "relayRejoin", "relayDropStart", "relayTakeover", "multiAct", "multiStart"]) pick(n);
    for (const n of ["leaveSession", "stopSession"]) leave(n);
    const act = W.sessionAct;
    if (act) W.sessionAct = (instance, kind) => { W._left = kind !== "resume" ? W._left : false; return act(instance, kind); };

    // Rule 1: a session that comes up after this device left stays off this screen.
    const started = W._relayHandlers.session_started;
    W.relayOn("session_started", (msg) => {
      if (W._left) {
        if (W.rlog) W.rlog("session came up after this device left it — not showing it; detaching");
        W.relaySendMsg({ type: "session_detach" });
        if (W.pc) { try { W.pc.close(); } catch (_) {} W.pc = null; }
        return;
      }
      if (started) return started(msg);
    });
  }, 0);

  // Rule 2: one history entry per time the session is shown.
  let pushed = false;
  W.sessionShown = (on) => {
    if (on && !pushed) {
      pushed = true;
      try { history.pushState({ wadoSession: true }, ""); } catch (_) {}
    } else if (!on && pushed) {
      pushed = false;
      // Left by other means (Leave, End): take our entry off, so Back is not spent on nothing.
      try { if (history.state && history.state.wadoSession) history.back(); } catch (_) {}
    }
  };
  addEventListener("popstate", (e) => {
    if (!pushed) return;
    // A sheet over the session (the trackpad's settings) pops back *to* our entry: still in.
    if (e.state && e.state.wadoSession) return;
    if (e.state && (e.state.wadoPad || e.state.wadoPadSettings)) return;
    pushed = false;
    emit({ type: "sessionBack" });
  });

  // Rule 3: the crumb learns that a picture was showing.
  document.addEventListener("loadeddata", (e) => {
    if (!e.target || e.target.id !== "wado-video") return;
    try {
      const v = JSON.parse(localStorage.getItem("wado.watching") || "null");
      if (v && !v.pic) { v.pic = true; localStorage.setItem("wado.watching", JSON.stringify(v)); }
    } catch (_) {}
  }, true);
})();
