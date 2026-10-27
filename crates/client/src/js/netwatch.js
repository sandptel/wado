// wado bridge — network watch. One job: notice that this device's network changed, get the
// session back fast, and measure how long that took (target < 2 s to picture behind a
// "Reconnecting…" overlay — Decision Log 2026-10-27).
//
// Before this nothing listened. A Wi-Fi → 5G switch was found by the relay link's 45 s silence
// watchdog, or by ICE declaring `failed` tens of seconds later, and only then rebuilt.
//
// Triggers:
//   * `online` / `navigator.connection` `change` — the browser saw the network change. Both the
//     relay socket and the peer connection are bound to an address the device no longer has, so
//     the link is redialled at once (`W.relayKick`) and the session's `session_started` rebuilds
//     the peer connection even if it still claims to be connected (`W._rebuildOnStart`).
//   * the peer connection sits in `disconnected` for DISCONNECT_GRACE_MS while the relay link is
//     up — the media path died under a link that is fine: re-offer over it.
//
// While one is in flight `W.recovery` is set; `link_state.js` reports "reconnecting" and the
// stage shows its overlay. It ends when the picture is back (or the link, with no session), and
// the time is logged — here and, through `rlog`, in the daemon's log, which is the number to
// read after a real Wi-Fi → 5G handoff.

const DISCONNECT_GRACE_MS = 1500;
const RECOVERY_GIVE_UP_MS = 20000;
W.recovery = null;          // {since, why, pc, kicked}
W.lastRecovery = null;      // {ms, why} — reported to the daemon by device_status.js

function beginRecovery(why, kick) {
  if (!W._relayTarget) return;   // direct mode, or not dialled: nothing of ours to recover
  const r = W.recovery || (W.recovery = { since: performance.now(), why, pc: W.pc, kicked: false });
  if (W.rlog) W.rlog("network: " + why + " — reconnecting");
  if (kick || !W.relayUp) {
    r.kicked = true;
    W._rebuildOnStart = W.sessionOn;
    // Joined right now means the seat is ours, even if its socket is already dead.
    W.relayKick(W.relayUp);
  } else if (W.sessionOn) {
    W.reconnectWebRTC().catch(() => W.handleFailure());
  }
  if (W.linkTick) W.linkTick();
}

function endRecovery(ok) {
  const r = W.recovery;
  if (!r) return;
  W.recovery = null;
  W._rebuildOnStart = false;
  const ms = Math.round(performance.now() - r.since);
  if (ok) {
    W.lastRecovery = { ms, why: r.why };
    if (W.rlog) W.rlog("network: reconnected in " + ms + " ms after " + r.why);
  } else if (W.rlog) {
    W.rlog("network: still not back " + ms + " ms after " + r.why + " — the slower retries carry on");
  }
  if (W.linkTick) W.linkTick();
}

// Back means: the link is up and, with a session, a peer connection is carrying it — a new one,
// or the old one after it was seen to drop (a connection that never noticed is not proof).
function settle() {
  const r = W.recovery;
  if (!r) return;
  const pc = W.pc;
  if (r.pc && r.pc.connectionState !== "connected") r.sawDown = true;
  const media = !W.sessionOn ||
    (pc && pc.connectionState === "connected" && (pc !== r.pc || (r.sawDown && !r.kicked)));
  if (W.relayUp && media) endRecovery(true);
  else if (performance.now() - r.since > RECOVERY_GIVE_UP_MS) endRecovery(false);
}

let changeTimer = null;
function networkChanged(why) {
  // A handoff fires several events within a few hundred ms; one recovery covers them.
  if (changeTimer) clearTimeout(changeTimer);
  changeTimer = setTimeout(() => { changeTimer = null; beginRecovery(why, true); }, 300);
}

window.addEventListener("online", () => networkChanged("network back"));
window.addEventListener("offline", () => { if (W.linkTick) W.linkTick(); });
try {
  const c = navigator.connection;
  if (c && c.addEventListener) {
    let type = c.type;
    c.addEventListener("change", () => {
      // `change` also fires for a new effectiveType on the same network; only a new network
      // (or a type the browser does not name) is worth dropping a working link for.
      if (c.type && c.type === type) return;
      networkChanged("network changed" + (type && c.type ? " (" + type + " → " + c.type + ")" : ""));
      type = c.type;
    });
  }
} catch (_) {}

let downSince = 0;
setInterval(() => {
  settle();
  const pc = W.pc;
  if (W.sessionOn && W.relayUp && !W.recovery && pc && pc.connectionState === "disconnected") {
    if (!downSince) downSince = performance.now();
    else if (performance.now() - downSince > DISCONNECT_GRACE_MS) {
      downSince = 0;
      beginRecovery("media path lost", false);
    }
  } else {
    downSince = 0;
  }
}, 250);
