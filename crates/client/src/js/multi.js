// wado bridge — several devices in one session (Decision Log 2026-10-30). Join someone's
// session, take it over, end it, start one of your own on a busy computer — and be told, in plain
// words, when someone else does one of those to yours.
//
// Joining never takes a seat. The page re-dials the relay as a **joined room** (`control=1`,
// `strict=1`, the session's `instance`; relay caps `control` + `room_strict`) and says
// `room_hello{view}` first (relay_link.js); the daemon serves the room as one more participant.
// An older relay or daemon has none of this: Join is not offered, and Take over / End fall back
// to today's seat takeover (`sessions.js`).
//
// Nothing here retries by itself (I17): displaced is displaced until a person taps Rejoin.
//
//   W.multiCan()                         this relay can carry joined rooms
//   W.multiAct(instance, kind, config)   kind: "join" | "takeover" | "end"
//   W.multiStart()                       "Start new": speed test, then where the session goes
//   W.multiHome()                        back to this device's own seat
//   W.multiTest(intent)                  run the speed test on the link that is up

W._relayRoom = null;        // {instance, kind, config, asked}
W.multi = { list: [], you: 0 };

W.multiCan = () => {
  const c = W.relayCaps || [];
  return c.includes("control") && c.includes("room_strict");
};

const multiSay = (text) => emit({ type: "multiNote", text });

function stopStream() {
  if (W.pc) { try { W.pc.close(); } catch (_) {} W.pc = null; }
  if (W.detachStream) W.detachStream();
  if (W.stopStats) W.stopStats();
  if (W.presenceClear) W.presenceClear();
  W.sessionOn = false;
  W._relayWanted = false;
  W._relayResuming = false;
  try { localStorage.removeItem("wado.watching"); } catch (_) {}
  emit({ type: "sessionOff" });
}

// Back to this device's own seat — the daemon it came from, or whichever the relay hands it.
W.multiHome = () => {
  const t = W._relayTarget;
  const was = W._relayRoom;
  W._relayRoom = null;
  W.multi = { list: [], you: 0 };
  emit({ type: "participants", list: [], you: 0, event: "", who: "" });
  emit({ type: "multiBusy", instance: "", kind: "" });
  if (!t || !was) return;
  W.relayDrop(true);
  W.relayDial(t.url, t.id);
  setTimeout(() => W.sessionsGet && W.sessionsGet(), 1500);
};

function dialRoom(instance, kind, config) {
  const t = W._relayTarget;
  if (!t) return false;
  if (W.sessionOn) stopStream();
  W._relayRoom = { instance, kind, config: config || null, asked: false };
  W._relayTakenOver = false;
  emit({ type: "multiBusy", instance, kind });
  W.relayDrop(true);
  W.relayDial(t.url, t.id);
  return true;
}

W.multiAct = (instance, kind, config) => {
  if (!W.multiCan()) {
    // An older relay: no joined rooms. Taking over and ending go the way they always went — a
    // tap that moves the seat — and joining is not something it can do at all.
    if (kind === "join") {
      multiSay("This relay is older than joining — update it, or take the session over instead");
      return;
    }
    W.sessionAct(instance, kind === "end" ? "stop" : "resume");
    return;
  }
  dialRoom(instance, kind, config);
};

// A refusal and a warning, as a sheet. Resolves true to go ahead.
function verdictSheet(v, kind) {
  return new Promise((done) => {
    const why = (v.reasons && v.reasons[0]) || "the link test failed";
    if (v.level === "refuse") {
      W.sheet.ask("multi", "Not possible on this link — " + why + ".", [
        { label: "OK", primary: true, run: () => done(false) },
      ]);
      return;
    }
    const go = { join: "Join anyway", takeover: "Take over anyway", start: "Start anyway" }[kind] || "Go ahead";
    W.sheet.ask("multi", "It will work, but " + why + ".", [
      { label: go, primary: true, run: () => done(true) },
      { label: "Cancel", run: () => done(false) },
    ]);
  });
}

// The speed test, said: the verdict goes to the Sessions section; a refusal stops here.
W.multiTest = async (intent) => {
  const v = await W.linkTest(intent);
  if (!v || v.level === "ok") return true;
  return verdictSheet(v, intent);
};

// The joined room is up (relay.js hands it here instead of its own link-up logic).
W.multiUp = async () => {
  const room = W._relayRoom;
  if (!room) return;
  // Back after a blip with the picture still ours: rejoin, which displaces nobody.
  if (room.asked) {
    if (W.sessionOn) {
      if (W.rlog) W.rlog("joined room back — rejoining the session");
      W.relaySendMsg({ type: "session_join" });
    }
    return;
  }
  room.asked = true;
  if (room.kind !== "end") {
    const ok = await W.multiTest("join");
    if (W._relayRoom !== room) return;
    if (!ok) { W.multiHome(); return; }
  }
  if (room.config) W._relayConfig = room.config;
  if (room.kind === "join") {
    W.relaySendMsg({ type: "session_join" });
  } else if (room.kind === "takeover") {
    W.relaySendMsg({ type: "session_takeover", config: room.config || W._relayConfig });
  } else {
    W.relaySendMsg({ type: "session_stop" });
  }
};

// A join, take over or end refused on a joined room: say why, and go home. True: handled.
W.multiError = (why) => {
  if (!W._relayRoom || W.sessionOn) return false;
  W.sheet.ask("multi", why.charAt(0).toUpperCase() + why.slice(1), [
    { label: "OK", primary: true, run: W.multiHome },
  ]);
  emit({ type: "multiBusy", instance: "", kind: "" });
  return true;
};

// Taken out of the session by someone else. Said plainly; Rejoin is a join and displaces nobody.
W.relayOn("session_left", (m) => {
  const by = m.by || "Another device";
  stopStream();
  const text = {
    taken_over: by + " took over this session",
    ended: (m.by ? m.by : "Someone") + " ended this session for everyone",
    opened_here_again: "This session was opened again in another tab of this device",
    stale: "Your connection dropped for too long, so you left the session",
  }[m.reason] || "You left this session";
  if (W.rlog) W.rlog("session_left: " + m.reason + " by " + (m.by || "?"));
  multiSay(text);
  const room = !!W._relayRoom;
  const rejoin = () => {
    if (room) W.relaySendMsg({ type: "session_join" });
    else W.relaySendMsg({ type: "session_rejoin" });
  };
  const canRejoin = m.reason === "taken_over" || m.reason === "stale";
  W.sheet.ask("multi", text + ".", canRejoin
    ? [{ label: "Rejoin", primary: true, run: rejoin }, { label: "Home", run: W.multiHome }]
    : [{ label: "OK", primary: true, run: W.multiHome }]);
});

// Who is in the session, and what just happened — a toast to everyone else.
W.relayOn("participants", (m) => {
  W.multi = { list: m.list || [], you: m.you || 0 };
  emit({ type: "participants", list: W.multi.list, you: W.multi.you, event: m.event || "", who: m.who || "" });
  if (W.presenceRoster) W.presenceRoster(W.multi.list, W.multi.you);
  const me = W.multi.list.find((p) => p.id === W.multi.you);
  if (!m.who || (me && me.name === m.who && m.event !== "left")) return;
  const text = {
    joined: m.who + " joined — they see and control this desktop too",
    left: m.who + " left the session",
    taken_over: m.who + " took this session over",
  }[m.event];
  if (text) multiSay(text);
});

// "Start new": the speed test, then where the new session goes — this daemon if it is idle, an
// idle sibling, or one the daemon starts (`server::desks`). `multiGo` hands back to the Rust
// side's ordinary Start.
let deskWait = 0;
W.multiStart = async () => {
  if (!W.relayUp) { emit({ type: "multiGo", fresh: true }); return; }
  const ok = await W.multiTest("start");
  if (!ok) return;
  clearTimeout(deskWait);
  deskWait = setTimeout(() => { deskWait = 0; emit({ type: "multiGo", fresh: true }); }, 15000);
  emit({ type: "multiBusy", instance: "", kind: "start" });
  W.relaySendMsg({ type: "desk_request" });
};
// An older daemon: no `desk_request`. Do what Start always did.
W.multiOld = () => {
  if (!deskWait) return;
  clearTimeout(deskWait); deskWait = 0;
  emit({ type: "multiGo", fresh: true });
};
W.relayOn("session_elsewhere", (m) => {
  if (!deskWait) return;
  clearTimeout(deskWait); deskWait = 0;
  emit({ type: "multiBusy", instance: "", kind: "" });
  const t = W._relayTarget;
  const here = W.pool && W.pool.instance;
  if (t && m.instance && m.instance !== here) {
    W.rememberInstance(String(t.id).replace(/[\s-]/g, ""), m.instance);
    if (W.rlog) W.rlog("new session goes to " + m.instance);
  }
  emit({ type: "multiGo", fresh: false });
});
W.relayOn("desks_full", (m) => {
  if (!deskWait) return;
  clearTimeout(deskWait); deskWait = 0;
  emit({ type: "multiBusy", instance: "", kind: "" });
  W.sheet.ask("multi", m.message || ("All " + m.max + " sessions are in use — end one, or join one."), [
    { label: "OK", primary: true },
  ]);
});

// Ended from a joined room: back to this device's own seat.
{
  const prev = W._relayHandlers["session_stopped"];
  W.relayOn("session_stopped", (m) => {
    if (prev) prev(m);
    if (W._relayRoom) {
      multiSay("Session ended for everyone");
      W.multiHome();
    }
  });
}

// Leaving a joined session goes home to this device's own seat.
{
  const leave = W.leaveSession;
  W.leaveSession = () => {
    leave();
    if (W._relayRoom) setTimeout(W.multiHome, 300);
  };
}
