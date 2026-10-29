// wado bridge — the link badge. One job: say which path this device is on right now, so the UI
// can show it at all times (Decision Log 2026-10-27: the limited relay lane must never be
// mistaken for the product, so the path is always on screen).
//
//   offline           this device has no network, or the relay does not answer
//   computer_offline  the relay is up, but no daemon is registered for this Remote ID
//   waiting           parked by the relay for another reason (a held desktop): `why`
//   ready             joined, no session: nothing needs a peer connection yet (files make their own)
//   connecting        a session whose peer connection is still being set up
//   relayed           a session whose peer connection failed or dropped: only small messages flow
//   reconnecting      a recovery is in flight (netwatch.js)
//   direct            a peer connection carries the session; `path` lan | internet | turn
//
// Emitted to Rust as {type:"link", …} on change and with a fresh RTT every 2 s, and kept as
// `W.linkNow` for device_status.js. `nat` is the computer's own NAT verdict (`host_net`).

W.linkNow = { state: "offline", path: "", rtt_ms: null, net: "", nat: "", turn: false };
W._relayWaitingNow = "";    // the relay's reason while it parks this join

// The selected candidate pair, from the transport's own pointer. The stats loop's "any
// succeeded pair" is not the same thing: after a re-route the last pair it sees can be stale.
async function selectedPath(pc) {
  const st = await pc.getStats();
  let pair = null;
  st.forEach((r) => { if (r.type === "transport" && r.selectedCandidatePairId) pair = st.get(r.selectedCandidatePairId); });
  if (!pair) st.forEach((r) => { if (r.type === "candidate-pair" && r.nominated && r.state === "succeeded") pair = r; });
  if (!pair) return null;
  const l = st.get(pair.localCandidateId), rm = st.get(pair.remoteCandidateId);
  const types = [l && l.candidateType, rm && rm.candidateType];
  // ponytail: host↔host is "lan"; a LAN path found through prflx reads as "internet". The
  // candidate addresses would settle it, and are not worth parsing for a label.
  const path = types.includes("relay") ? "turn" : (types[0] === "host" && types[1] === "host") ? "lan" : "internet";
  const rtt = pair.currentRoundTripTime;
  return { path, rtt_ms: rtt == null ? null : Math.round(rtt * 1000) };
}

function netType() {
  try {
    const c = navigator.connection;
    return (c && (c.type || c.effectiveType)) || "";
  } catch (_) { return ""; }
}

let last = "";
W.linkTick = async () => {
  const now = { state: "offline", path: "", rtt_ms: null, net: netType(), nat: W.linkNow.nat, turn: W.linkNow.turn };
  const pc = W.pc;
  if (!navigator.onLine) now.state = "offline";
  else if (W.recovery && W.sessionOn) now.state = "reconnecting";
  else if (pc && pc.connectionState === "connected") {
    now.state = "direct";
    try { Object.assign(now, (await selectedPath(pc)) || {}); } catch (_) {}
  } else if (W.relayUp && !W.sessionOn) now.state = "ready";
  else if (W.relayUp) {
    // Being set up is not a failure: only a connection that failed, dropped or never came is.
    const st = pc ? pc.connectionState : "new";
    now.state = st === "new" || st === "connecting" ? "connecting" : "relayed";
  }
  else if (W._relayWaitingNow) {
    now.state = /no computer/i.test(W._relayWaitingNow) ? "computer_offline" : "waiting";
    now.why = W._relayWaitingNow;
  }
  else if (W.recovery) now.state = "reconnecting";
  // "offline" has two causes and only one is this device's: say which.
  if (now.state === "offline") now.why = navigator.onLine ? "relay_unreachable" : "no_network";
  W.linkNow = now;
  const key = JSON.stringify(now);
  if (key !== last) { last = key; emit(Object.assign({ type: "link" }, now)); }
};
setInterval(W.linkTick, 2000);

// The relay parks a join while no daemon is registered; that is "computer offline", not "offline".
{
  const prev = W._relayHandlers.__waiting;
  W.relayOn("__waiting", (m) => { W._relayWaitingNow = (m && m.reason) || "waiting"; if (prev) prev(m); W.linkTick(); });
  const up = W._relayHandlers.__up;
  W.relayOn("__up", (m) => {
    if (up) up(m);
    W._relayWaitingNow = "";
    W.relaySendMsg({ type: "host_net_request" });
    W.linkTick();
  });
  const down = W._relayHandlers.__down;
  W.relayOn("__down", (m) => { if (down) down(m); W._relayWaitingNow = ""; W.linkTick(); });
  W.relayOn("host_net", (m) => {
    const n = (m && m.net) || {};
    W.linkNow.nat = n.nat || "";
    W.linkNow.turn = !!n.turn;
    last = "";
    W.linkTick();
  });
}
