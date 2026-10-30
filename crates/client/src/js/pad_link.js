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
//   W.pad.bin                 the daemon reads compact touch frames (its pong said so)
//
// Compact touch frames (wado_protocol::pad_bin): `seq u32 · t u32 · n × (id u8, x u16, y u16)`,
// big-endian — ~4× smaller than the JSON. Every ping offers them (`bin: true`); only a daemon
// whose pong says `bin` back is sent one, so an older daemon keeps getting JSON. Direct, a frame
// is a binary message on `pad-touch`; relayed, `pad_in {k:"bin", b:<base64>}` (the relay link is
// text).
//
//   W.padBin.encode(frame) → Uint8Array      W.padBin.decode(bytes) → frame | null

(() => {
  const B = (W.padBin = {});
  B.encode = (m) => {
    const c = (m.c || []).slice(0, 10);
    const b = new Uint8Array(8 + 5 * c.length);
    const v = new DataView(b.buffer);
    v.setUint32(0, m.seq >>> 0);
    v.setUint32(4, m.t >>> 0);
    c.forEach(([id, x, y], i) => { v.setUint8(8 + 5 * i, id); v.setUint16(9 + 5 * i, x); v.setUint16(11 + 5 * i, y); });
    return b;
  };
  B.decode = (b) => {
    if (b.length < 8 || (b.length - 8) % 5 || (b.length - 8) / 5 > 10) return null;
    const v = new DataView(b.buffer, b.byteOffset, b.byteLength);
    const c = [];
    for (let o = 8; o < b.length; o += 5) c.push([v.getUint8(o), v.getUint16(o + 1), v.getUint16(o + 3)]);
    return { k: "frame", seq: v.getUint32(0), t: v.getUint32(4), c };
  };
  B.b64 = (b) => { let s = ""; for (const x of b) s += String.fromCharCode(x); return btoa(s); };
})();

(() => {
  const P = (W.pad = { state: "off", rtt: null, bin: false });
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
    if (m.k === "pong") {
      P.rtt = ((now32() - m.t) >>> 0);
      if (m.bin && !P.bin) { P.bin = true; rlog("compact touch frames on"); }
    }
    for (const fn of msgs) { try { fn(m); } catch (_) {} }
    // The daemon closed the pad (no grant, no /dev/uinput): stop sending into it.
    if (m.k === "err" && m.fatal) stop(false);
  }

  // The path changed: the daemon's picture of this pad (size, fingers, held keys) is re-sent whole.
  function resend() {
    if (W.padTouch && W.padTouch.resend) W.padTouch.resend();
    if (W.padKeys && W.padKeys.resend) W.padKeys.resend();
  }

  const open = (dc) => dc && dc.readyState === "open";

  P.send = (msg, lossy) => {
    if (P.state === "off") return false;
    const bin = P.bin && msg && msg.k === "frame" ? W.padBin.encode(msg) : null;
    if (P.state === "direct") {
      const dc = lossy ? touch : keys;
      if (open(dc)) { dc.send(bin || JSON.stringify(msg)); return true; }
    }
    const out = bin ? { k: "bin", b: W.padBin.b64(bin) } : msg;
    return !!(W.relaySendMsg && W.relaySendMsg({ type: "pad_in", msg: out }));
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
    if (want) { setState("relayed"); resend(); }
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
      if (pc !== me || !want) return; // closed while gathering: offer nothing
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
        deliver({ k: "err", message: answer.err, fatal: true });
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
      resend();
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
    P.bin = false; // until this daemon's pong says it reads them
    setState("relayed");
    pingTimer = setInterval(() => P.send({ k: "ping", t: now32(), bin: true }, false), 2000);
    P.send({ k: "ping", t: now32(), bin: true }, false);
    tryDirect();
  };

  // `bye`: tell the daemon to let go of everything and remove its devices. Through the relay,
  // because closing the peer connection below can drop what is still queued on its channels.
  function stop(bye) {
    if (bye && want && W.relaySendMsg) W.relaySendMsg({ type: "pad_in", msg: { k: "bye" } });
    want = false;
    clearInterval(pingTimer);
    pingTimer = null;
    if (pc) { try { pc.close(); } catch (_) {} }
    pc = touch = keys = null;
    const w = answerWait; // settle an offer still waiting, or `offering` would stay stuck
    answerWait = null;
    if (w) w({ err: "" });
    setState("off");
  }
  P.stop = () => stop(true);
})();
