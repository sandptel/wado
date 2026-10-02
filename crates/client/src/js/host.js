// wado bridge — the computer itself: its sound, Wi-Fi, Bluetooth and sleep (server::host).
//
// Needs only the relay link, not a session. Polled while the control centre is open — that is
// when someone is looking — and pushed back by the daemon after every action.

W.relayOn("host_state", (msg) => {
  W.hostState = msg.state || null;
  emit({ type: "hostState", state: W.hostState });
  const fast = !!(W.hostState && (W.hostState.media || []).some((p) => p.playing));
  if (fast !== hostFast) { hostFast = fast; hostArm(); }
});
W.relayOn("host_error", (msg) => emit({ type: "captureNote", text: msg.message || "That did not work." }));

W.hostGet = () => W.relaySendMsg && W.relayUp && W.relaySendMsg({ type: "host_get" });
W.hostDo = (action) => W.relaySendMsg({ type: "host_do", action });

// Quicker while something plays, so the playback card keeps step with the player.
let hostTimer = 0, hostOn = false, hostFast = false;
const hostArm = () => {
  clearInterval(hostTimer);
  if (hostOn) hostTimer = setInterval(W.hostGet, hostFast ? 1200 : 2500);
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
  W.relayOn("__up", (m) => { if (up) up(m); W.hostGet(); });
}
