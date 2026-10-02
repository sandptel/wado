// wado bridge — the interface's own watchdog.
//
// The picture and the input live in JS; every button lives in the Rust/WASM UI. When that UI
// dies — a Rust panic, a WASM trap — the video keeps playing and every button goes dead, which
// is exactly "all the buttons stop working mid-session". Nothing inside a dead WASM instance
// can bring it back, so the page reloads itself and, if a session was on, takes it straight
// back (`wado.crashRejoin`, read by relayResumeIfWatching). What happened is kept for the next
// load, shown as a note and sent to the daemon's log, so the cause can be found.
//
// Two detectors: the panic hook (main.rs calls W.crashed with the message), and a heartbeat for
// what a hook cannot see — a trap, or a UI that stopped answering at all.
W.crashed = (why) => {
  if (W._crashing) return;
  W._crashing = true;
  const text = String(why || "unknown").slice(0, 2000);
  try { localStorage.setItem("wado.lastCrash", JSON.stringify({ t: Date.now(), why: text })); } catch (_) {}
  try { if (W.sessionOn) sessionStorage.setItem("wado.crashRejoin", "1"); } catch (_) {}
  try { W.rlog("UI crashed — reloading: " + text.slice(0, 400)); } catch (_) {}
  setTimeout(() => location.reload(), 400);
};

{
  let sent = 0, got = 0;
  W.alive = (n) => { got = Math.max(got, n); };
  setInterval(() => {
    // A hidden tab's timers are throttled and its UI may legitimately not answer.
    if (document.visibilityState !== "visible") { sent = got; return; }
    // Counting starts at the first answer: before it, the UI is still mounting.
    if (got && sent - got >= 5) { W.crashed("the interface stopped answering for 10 s"); return; }
    emit({ type: "alive", n: ++sent });
  }, 2000);
}

// The note from last time, once the UI and the link are up.
{
  let last = null;
  try { last = JSON.parse(localStorage.getItem("wado.lastCrash") || "null"); localStorage.removeItem("wado.lastCrash"); } catch (_) {}
  if (last && Date.now() - last.t < 120000) {
    setTimeout(() => emit({ type: "captureNote", text: "wado recovered from a problem and reloaded." }), 2500);
    const up = W._relayHandlers.__up;
    let told = false;
    W.relayOn("__up", (m) => {
      if (up) up(m);
      if (!told) { told = true; try { W.rlog("previous page crashed: " + last.why.slice(0, 1500)); } catch (_) {} }
    });
  }
}
