// wado bridge — the servers a direct path gathers with: the computer's answer to `ice_get` (STUN,
// and TURN with credentials it minted — the way through when both ends sit behind a VPN), kept
// until they near expiry; plain STUN when it does not answer in time (an older daemon).
// After relay_link.js (it chains `__up`).
(() => {
  const STUN = [{ urls: "stun:stun.l.google.com:19302" }, { urls: "stun:stun.cloudflare.com:3478" }];
  let got = null, until = 0, pending = null, settle = null;
  W.relayOn("ice", (m) => {
    if (Array.isArray(m.servers) && m.servers.length) {
      got = m.servers;
      until = Date.now() + 800 * (m.ttl | 0); // renewed at 80% of the ttl
    }
    if (settle) settle();
    pending = settle = null;
  });
  const prevUp = W._relayHandlers["__up"];
  W.relayOn("__up", (m) => { until = 0; if (prevUp) prevUp(m); });

  /** For `new RTCPeerConnection({ iceServers: await W.iceServers() })`. */
  W.iceServers = async () => {
    if (got && Date.now() < until) return got;
    if (!pending) {
      pending = new Promise((r) => { settle = r; });
      if (!(W.relaySendMsg && W.relaySendMsg({ type: "ice_get" }))) pending = settle = null;
    }
    if (pending) await Promise.race([pending, new Promise((r) => setTimeout(r, 1500))]);
    return got && Date.now() < until ? got : STUN;
  };
})();
