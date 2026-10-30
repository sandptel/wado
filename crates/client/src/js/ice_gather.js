// wado bridge — when an offer has enough candidates to go. One job: stop waiting for ICE
// gathering to *finish* once what a connection needs is in hand, and trickle the rest.
//
// Measured 2026-10-30 (scripts/reconnect-time.mjs, a release daemon over a local relay): every
// reconnect spent ~3 s here — the host candidate in 25 ms, the server-reflexive one in 75 ms,
// and then nothing until the old 3 s cap, because "complete" waits for every STUN server and
// every interface to answer or time out. That wait was most of the time to picture, against a
// < 2 s target (Decision Log 2026-10-27). So:
//
//   • a server-reflexive or relay candidate in hand, and SETTLE_MS without another → go
//   • only host candidates, HOST_ONLY_MS after the first → go (a LAN, or STUN blocked)
//   • gathering complete → go;  CAP_MS → go with whatever there is (as before)
//
// What arrives after the offer is not lost: `W.iceTrickle` sends each later candidate as
// `ice_candidate`, which every daemon since September adds to the session's peer connection.
//
//   W.iceGather(pc) → Promise<{why, ms, n}>   resolve when the offer may go
//   W.iceTrickle(pc, send)                    send(candidateJSON) for every later candidate

(() => {
  const SETTLE_MS = 60;
  const HOST_ONLY_MS = 700;
  const CAP_MS = 3000;

  W.iceGather = (pc) => new Promise((resolve) => {
    const t0 = performance.now();
    let n = 0, reflexive = false, settle = 0, hostOnly = 0, done = false;
    const finish = (why) => {
      if (done) return;
      done = true;
      clearTimeout(settle); clearTimeout(hostOnly); clearTimeout(cap);
      pc.removeEventListener("icecandidate", onCand);
      pc.removeEventListener("icegatheringstatechange", onState);
      resolve({ why, ms: Math.round(performance.now() - t0), n });
    };
    const onCand = (ev) => {
      if (!ev.candidate) return finish("complete");
      n++;
      const typ = (/ typ (\w+)/.exec(ev.candidate.candidate || "") || [])[1];
      if (typ === "srflx" || typ === "relay" || typ === "prflx") reflexive = true;
      if (reflexive) {
        clearTimeout(settle);
        settle = setTimeout(() => finish("reflexive"), SETTLE_MS);
      } else if (!hostOnly) {
        hostOnly = setTimeout(() => finish("host only"), HOST_ONLY_MS);
      }
    };
    const onState = () => { if (pc.iceGatheringState === "complete") finish("complete"); };
    const cap = setTimeout(() => finish("cap"), CAP_MS);
    if (pc.iceGatheringState === "complete") return finish("complete");
    pc.addEventListener("icecandidate", onCand);
    pc.addEventListener("icegatheringstatechange", onState);
  });

  W.iceTrickle = (pc, send) => {
    pc.addEventListener("icecandidate", (ev) => {
      if (!ev.candidate || !ev.candidate.candidate) return;
      try { send(JSON.stringify(ev.candidate)); } catch (_) {}
    });
  };
})();
