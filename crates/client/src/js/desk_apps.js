// wado bridge — a Mac's own apps, with no session (`desk_apps`; ui/computer/desk): asked for every
// 3 s while they are on screen, acted on (`app_do`), and how their icons take a press.
//
// The list and its icons arrive the way the session's app drawer gets them — relay.js's
// `apps_list` / `apps_icons_list`, one icon cache — so nothing here keeps a list of its own.
//
//   W.deskAppsWatch(on)               ask now and every 3 s (not while the page is hidden), or stop
//   W.deskAppDo(exec, action, running) open | hide | unhide | quit | force_quit; the daemon answers
//                                      with a fresh `apps_list`, or `host_error` (a toast, host.js)
//
// Rust draws the bounce of an app just opened from `deskLaunching`: the apps tapped open that the
// list does not show running yet, each for at most 10 s.

let deskTimer = 0;
const deskAsk = () => { if (W.relayUp && !document.hidden) W.relaySendMsg({ type: "apps_request", lean: true }); };
W.deskAppsWatch = (on) => {
  clearInterval(deskTimer);
  deskTimer = 0;
  if (on) { deskAsk(); deskTimer = setInterval(deskAsk, 3000); }
};
document.addEventListener("visibilitychange", () => { if (deskTimer) deskAsk(); });

const deskLaunching = new Map(); // exec → its 10 s timer
const deskTell = () => emit({ type: "deskLaunching", execs: [...deskLaunching.keys()] });
const deskLanded = (exec) => { clearTimeout(deskLaunching.get(exec)); deskLaunching.delete(exec); };

W.deskAppDo = (exec, action, running) => {
  if (!W.relayUp) { emit({ type: "captureNote", text: "Not connected to it right now." }); return; }
  W.relaySendMsg({ type: "app_do", exec, action });
  try { if (navigator.vibrate) navigator.vibrate(6); } catch (_) {}
  if (action === "open" && !running) {
    deskLanded(exec);
    deskLaunching.set(exec, setTimeout(() => { deskLaunching.delete(exec); deskTell(); }, 10000));
    deskTell();
  }
};

{
  const prev = W._relayHandlers.apps_list;
  W.relayOn("apps_list", (msg) => {
    if (prev) prev(msg);
    const n = deskLaunching.size;
    for (const a of msg.apps || []) if (a.running && deskLaunching.has(a.exec)) deskLanded(a.exec);
    if (deskLaunching.size !== n) deskTell();
  });
}

// ── A press on an icon (`data-dapp` = its exec) ─────────────────────────────
// Long-press on touch by a timer — iOS never sends `contextmenu` for one (as chrome.js) — and
// right-click with a mouse open its menu. The tap that ends a long press is swallowed, so it
// does not also open the app; a finger that moves (scrolling the dock) is not a press.
{
  let hold = 0, held = false, x0 = 0, y0 = 0;
  const tile = (e) => e.target && e.target.closest && e.target.closest("[data-dapp]");
  const menu = (b) => emit({ type: "deskAppMenu", exec: b.dataset.dapp });
  const cancel = () => { clearTimeout(hold); hold = 0; };
  document.addEventListener("pointerdown", (e) => {
    const b = tile(e);
    held = false;
    cancel();
    if (!b || e.pointerType === "mouse") return;
    x0 = e.clientX; y0 = e.clientY;
    hold = setTimeout(() => {
      hold = 0;
      held = true;
      try { if (navigator.vibrate) navigator.vibrate(12); } catch (_) {}
      menu(b);
    }, 420);
  }, true);
  document.addEventListener("pointerup", cancel, true);
  document.addEventListener("pointercancel", cancel, true);
  document.addEventListener("pointermove", (e) => {
    if (hold && Math.hypot(e.clientX - x0, e.clientY - y0) > 8) cancel();
  }, true);
  document.addEventListener("click", (e) => {
    if (held && tile(e)) { held = false; e.stopPropagation(); e.preventDefault(); }
  }, true);
  document.addEventListener("contextmenu", (e) => {
    const b = tile(e);
    if (!b) return;
    e.preventDefault();
    if (!held) menu(b);
  }, true);
}
