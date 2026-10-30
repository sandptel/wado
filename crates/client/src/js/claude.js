// wado bridge — Claude Code's sessions on the selected computer (daemon: crates/server/src/claude/).
//
//   W.claudeAnswer(session, ask, allow)  Allow (behind the passkey) or Deny (not: it can only stop)
//   W.claudeOpen(session)                its shell — attached to its tmux — behind the passkey
//   W.claudeDirs(path)                   the folders for the new-session picker ("" = home)
//   W.claudeNew(dir)                     start Claude in `dir`; its shell opens
//
// To Rust: {type:"claude", state} on every change; {type:"claudeDirs", dirs}.
// A notification's Allow / Deny (sw.js) lands here as a pending answer, given once its card shows.

(() => {
  const send = (o) => !!(W.relaySendMsg && W.relaySendMsg(o));
  let cards = { sessions: [], hooks: false, tmux: false };
  let pending = null; // {session, ask, act} from a notification

  W.claudeAnswer = (session, ask, allow) => {
    const go = () => send({ type: "claude_answer", session, ask, allow: !!allow });
    if (allow) W.passkeyThen("allow", go);
    else go();
  };
  W.claudeOpen = (session) => W.passkeyThen("claude", () => {
    send({ type: "claude_open", session });
    emit({ type: "claudeShell" });
  });
  W.claudeDirs = (path) => send({ type: "claude_dirs_get", path: path || "" });
  // The new-session picker, once unlocked (the folder walk is behind the passkey too).
  W.claudePick = () => { W.claudeDirs(""); emit({ type: "claudePicker" }); };
  W.claudeNew = (dir) => W.passkeyThen("new", () => {
    send({ type: "claude_new", dir });
    emit({ type: "claudeShell" });
  });

  function settle() {
    if (!pending) return;
    const c = (cards.sessions || []).find((s) => s.id === pending.session);
    if (!c) return;
    const p = pending;
    pending = null;
    if (!c.ask || c.ask.id !== p.ask) {
      emit({ type: "status", text: "That question was already answered." });
      return;
    }
    W.claudeAnswer(p.session, p.ask, p.act === "allow");
  }

  W.relayOn("claude_cards", (m) => {
    cards = m.state || cards;
    emit({ type: "claude", state: cards });
    settle();
  });
  W.relayOn("claude_dirs", (m) => emit({ type: "claudeDirs", dirs: m.dirs || {} }));
  {
    const up = W._relayHandlers.__up;
    W.relayOn("__up", (m) => { if (up) up(m); send({ type: "claude_get" }); });
  }

  // A notification's button, from the service worker (open page) or the address (cold start).
  W.claudePending = (p) => {
    if (!p || !p.session || !p.ask || (p.act !== "allow" && p.act !== "deny")) return;
    pending = p;
    settle();
  };
  try {
    const q = new URLSearchParams(location.search);
    if (q.get("ask")) {
      W.claudePending({ session: q.get("session"), ask: q.get("ask"), act: q.get("act") });
      for (const k of ["session", "ask", "act", "rid"]) q.delete(k);
      const rest = q.toString();
      history.replaceState(null, "", location.pathname + (rest ? "?" + rest : "") + location.hash);
    }
  } catch (_) {}
  if (navigator.serviceWorker) {
    navigator.serviceWorker.addEventListener("message", (e) => {
      const d = e.data || {};
      if (d.type === "wado-notify-click" && d.note) {
        W.claudePending({ session: d.note.session, ask: d.note.ask, act: d.act });
      }
    });
  }
})();
