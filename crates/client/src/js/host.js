// wado bridge — the computer itself: its sound, Wi-Fi, Bluetooth and sleep (server::host).
//
// Needs only the relay link, not a session. Polled while the control centre is open — that is
// when someone is looking — and pushed back by the daemon after every action.

// A poll in flight when an action is sent answers with the state from *before* the action, and
// it can land after the panel has already shown the change — flipping the toggle back, which
// reads as "the button did nothing". So polls are counted, the ones in flight at an action are
// dropped, and no poll goes out until the action's own answer is back.
let hostGets = 0, hostDos = 0, hostDrop = 0;
const hostPics = {};
W.relayOn("host_state", (msg) => {
  if (hostDrop > 0) { hostDrop--; hostGets = Math.max(0, hostGets - 1); return; }
  if (hostDos > 0) hostDos--;
  else hostGets = Math.max(0, hostGets - 1);
  W.hostState = msg.state || null;
  // Pictures come once per viewer (server::host::trim); a repeat says `*_same` — keep ours.
  for (const p of (W.hostState && W.hostState.media) || []) {
    const had = hostPics[p.bus] || {};
    if (p.art_same) p.art = had.art || null;
    if (p.icon_same) p.icon = had.icon || null;
    hostPics[p.bus] = { art: p.art, icon: p.icon };
  }
  emit({ type: "hostState", state: W.hostState });
  const fast = !!(W.hostState && (W.hostState.media || []).some((p) => p.playing));
  if (fast !== hostFast) { hostFast = fast; hostArm(); }
});
W.relayOn("host_error", (msg) => emit({ type: "captureNote", text: msg.message || "That did not work." }));

W.hostGet = () => {
  if (!W.relaySendMsg || !W.relayUp || hostDos > 0) return;
  hostGets++;
  W.relaySendMsg({ type: "host_get" });
};
W.hostDo = (action) => {
  hostDrop = hostGets;
  hostDos++;
  W.relaySendMsg({ type: "host_do", action });
  // An answer lost with a dropped link must not stop the polls for good.
  setTimeout(() => { hostDos = Math.max(0, hostDos - 1); }, 6000);
};

// Quicker while something plays, so the playback card keeps step with the player.
let hostTimer = 0, hostOn = false, hostFast = false;
const hostArm = () => {
  clearInterval(hostTimer);
  // During a session the downlink belongs to the picture: a slower refresh, whatever plays.
  if (hostOn) hostTimer = setInterval(W.hostGet, W.sessionOn ? 3000 : hostFast ? 1200 : 2500);
};
W.hostWatch = (on) => { hostOn = !!on; if (on) W.hostGet(); hostArm(); };

// "Play on this phone" with no session running: a peer connection carrying audio alone.
// With a session, its own connection already carries the sound and nothing is needed.
W.listenStart = async () => {
  if (W.sessionOn || !W.relayUp || (W.pc && W.pc.connectionState === "connected")) return;
  try { await W._relayNegotiate({ audioOnly: true }); } catch (e) { if (W.rlog) W.rlog("listen failed: " + e.message); }
};
W.listenStop = () => {
  if (W._listenOnly && W.pc) { try { W.pc.close(); } catch (_) {} W.pc = null; W._listenOnly = false; }
};

{
  const up = W._relayHandlers.__up;
  W.relayOn("__up", (m) => { if (up) up(m); hostGets = hostDos = hostDrop = 0; W.hostGet(); });
}
