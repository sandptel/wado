// wado bridge — the trackpad's link (wire: crates/protocol/src/pad.rs; daemon: crates/server/src/pad/).
//
// Relay first, then direct. `start()` is usable at once: messages go sealed through the relay
// link as `{type:"pad_in", msg}`. Meanwhile it negotiates the pad's own peer connection — two
// channels, `pad-touch` (unordered, no retransmits: a frame holds every finger, so only the
// newest matters) and `pad-keys` (ordered, reliable) — and moves over the moment both are open,
// re-sending the pad's size and current frame so the daemon's picture is whole on the new path.
// No direct path (ICE fails) means it simply stays on the relay; a network change tries again.
//
//   W.pad.start() / W.pad.stop()
//   W.pad.send(msg, lossy)    lossy → touch channel when direct; everything else → keys channel
//   W.pad.state               "off" | "relayed" | "direct"
//   W.pad.onState(fn), W.pad.onMsg(fn)   daemon replies: {k:"err", message}, …
//   W.pad.rtt                 ms, from a ping every 2 s

(() => {
  const P = (W.pad = { state: "off", rtt: null });
  const states = [];
  const msgs = [];
  let want = false;
  let pc = null, touch = null, keys = null;
  let offering = false;
  let answerWait = null;
  let pingTimer = null;

  const rlog = (s) => { if (W.rlog) W.rlog("pad: " + s); };
  const now32 = () => (performance.now() | 0) >>> 0;

  P.onState = (fn) => states.push(fn);
  P.onMsg = (fn) => msgs.push(fn);

  function setState(s) {
    if (P.state === s) return;
    P.state = s;
    for (const fn of states) { try { fn(s); } catch (_) {} }
  }

  function deliver(m) {
    if (!m || typeof m !== "object") return;
    if (m.k === "pong") { P.rtt = ((now32() - m.t) >>> 0); }
    for (const fn of msgs) { try { fn(m); } catch (_) {} }
  }

  const open = (dc) => dc && dc.readyState === "open";

  P.send = (msg, lossy) => {
    if (P.state === "off") return false;
    if (P.state === "direct") {
      const dc = lossy ? touch : keys;
      if (open(dc)) { dc.send(JSON.stringify(msg)); return true; }
    }
    return !!(W.relaySendMsg && W.relaySendMsg({ type: "pad_in", msg }));
  };

  W.relayOn("pad_out", (m) => deliver(m.msg));
  W.relayOn("pad_answer", (m) => { const w = answerWait; answerWait = null; if (w) w(m); });
  {
    const prevUp = W._relayHandlers["__up"];
    W.relayOn("__up", (m) => { if (prevUp) prevUp(m); if (want && P.state !== "direct") tryDirect(); });
  }

  function dropDirect(why) {
    if (!pc) return;
    rlog("direct link down — " + why);
    try { pc.close(); } catch (_) {}
    pc = touch = keys = null;
    if (want) setState("relayed");
  }

  async function tryDirect() {
    if (offering || !want || pc) return;
    offering = true;
    const me = new RTCPeerConnection({
      iceServers: [{ urls: "stun:stun.l.google.com:19302" }, { urls: "stun:stun1.l.google.com:19302" }],
    });
    pc = me;
    touch = me.createDataChannel("pad-touch", { ordered: false, maxRetransmits: 0 });
    keys = me.createDataChannel("pad-keys", { ordered: true });
    for (const dc of [touch, keys]) {
      dc.onmessage = (ev) => { try { deliver(JSON.parse(ev.data)); } catch (_) {} };
      dc.onclose = () => { if (pc === me) dropDirect("channel closed"); };
    }
    me.onconnectionstatechange = () => {
      if (pc === me && (me.connectionState === "failed" || me.connectionState === "closed")) dropDirect(me.connectionState);
    };
    try {
      await me.setLocalDescription(await me.createOffer());
      await new Promise((ok) => {
        if (me.iceGatheringState === "complete") return ok();
        const t = setTimeout(ok, 3000);
        me.onicegatheringstatechange = () => { if (me.iceGatheringState === "complete") { clearTimeout(t); ok(); } };
      });
      const answer = await new Promise((ok) => {
        answerWait = ok;
        if (!W.relaySendMsg({ type: "pad_offer", sdp: JSON.stringify(me.localDescription) })) {
          answerWait = null;
          ok({ err: "" }); // the relay link is down; stay relayed and retry when it is back
        }
        setTimeout(() => { if (answerWait === ok) { answerWait = null; ok({ err: "" }); } }, 15000);
      });
      if (pc !== me) return;
      if (answer.err) {
        // The daemon refused the pad itself (no `input` grant, not approved): say so and stop.
        deliver({ k: "err", message: answer.err });
        P.stop();
        return;
      }
      if (!answer.sdp) { dropDirect("no answer"); return; }
      await me.setRemoteDescription(JSON.parse(answer.sdp));
      await new Promise((ok, no) => {
        const t = setTimeout(() => no(new Error("no direct path (ICE)")), 8000);
        const check = () => { if (open(touch) && open(keys)) { clearTimeout(t); ok(); } };
        touch.onopen = check;
        keys.onopen = check;
        check();
      });
      if (pc !== me) return;
      setState("direct");
      rlog("direct link up");
      if (W.padTouch && W.padTouch.resend) W.padTouch.resend();
    } catch (e) {
      if (pc === me) dropDirect(String(e.message || e));
    } finally {
      offering = false;
    }
  }

  P.start = () => {
    if (want) return;
    want = true;
    P.rtt = null;
    setState("relayed");
    pingTimer = setInterval(() => P.send({ k: "ping", t: now32() }, false), 2000);
    P.send({ k: "ping", t: now32() }, false);
    tryDirect();
  };

  P.stop = () => {
    want = false;
    clearInterval(pingTimer);
    pingTimer = null;
    if (pc) { try { pc.close(); } catch (_) {} }
    pc = touch = keys = null;
    answerWait = null;
    setState("off");
  };
})();
