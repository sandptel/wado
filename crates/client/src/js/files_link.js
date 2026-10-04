// wado bridge — the file manager's connection (wire: crates/protocol/src/files.rs).
//
// A second RTCPeerConnection, opened on demand, with one ordered reliable data channel,
// "files". Separate from the session's so a transfer never shares an SCTP association with
// input (invariant #1). Its offer and answer travel sealed through the envelope, so the DTLS
// fingerprint it ends up trusting is the computer's.
//
//   W.files.connect()                → Promise, resolves once the channel is open
//   W.files.req(op, args)            → Promise of the first answer ({ok,…} resolves, {err} rejects)
//   W.files.stream(op, args, h)      → id; h.msg(m) for every text answer, h.bytes(u8) per chunk
//   W.files.bytes(id, u8)            → send one upload chunk (4-byte id + data)
//   W.files.onDown(fn)               → fn() whenever the channel is lost

(() => {
  const F = (W.files = W.files || {});
  F.pc = null;
  F.dc = null;
  F.ready = false;
  F.info = null; // hello: { roots, home, access, device }
  let nextId = 1;
  let connecting = null;
  let answerWait = null;
  const handlers = new Map(); // id → { msg, bytes }
  const downs = [];

  const rlog = (s) => { if (W.rlog) W.rlog("files: " + s); };

  F.onDown = (fn) => downs.push(fn);

  function lost(why) {
    if (!F.pc && !F.ready) return;
    rlog("link lost — " + why);
    F.ready = false;
    try { if (F.pc) F.pc.close(); } catch (_) {}
    F.pc = null;
    F.dc = null;
    connecting = null;
    const hs = [...handlers.values()];
    handlers.clear();
    for (const h of hs) { try { h.msg({ err: "the connection to the computer was lost", lost: true }); } catch (_) {} }
    for (const fn of downs) { try { fn(); } catch (_) {} }
  }
  F.close = () => lost("closed");

  W.relayOn("files_answer", (msg) => {
    const w = answerWait;
    answerWait = null;
    if (w) w(msg);
  });
  // The relay link dropping takes the envelope with it; the files connection may survive on
  // its own path, but nothing new can be negotiated until the link is back.
  {
    const prev = W._relayHandlers["__down"];
    W.relayOn("__down", (m) => { if (prev) prev(m); if (answerWait) { const w = answerWait; answerWait = null; w({ err: "the link to the computer dropped" }); } });
  }

  async function open() {
    if (!W.e2eReady) throw new Error("not connected to the computer yet");
    const pc = new RTCPeerConnection({
      iceServers: [{ urls: "stun:stun.l.google.com:19302" }, { urls: "stun:stun1.l.google.com:19302" }],
    });
    const dc = pc.createDataChannel("files", { ordered: true });
    dc.binaryType = "arraybuffer";
    F.pc = pc;
    F.dc = dc;
    pc.onconnectionstatechange = () => {
      if (pc === F.pc && (pc.connectionState === "failed" || pc.connectionState === "closed")) lost(pc.connectionState);
    };
    dc.onclose = () => { if (dc === F.dc) lost("channel closed"); };
    dc.onmessage = (ev) => {
      if (typeof ev.data === "string") {
        let m;
        try { m = JSON.parse(ev.data); } catch (_) { return; }
        const h = handlers.get(m.id);
        if (h) h.msg(m);
        return;
      }
      const u8 = new Uint8Array(ev.data);
      if (u8.length < 4) return;
      const id = new DataView(ev.data).getUint32(0);
      const h = handlers.get(id);
      if (h && h.bytes) h.bytes(u8.subarray(4));
    };

    await pc.setLocalDescription(await pc.createOffer());
    // Non-trickle, like the session's: wait for gathering, bounded.
    await new Promise((ok) => {
      if (pc.iceGatheringState === "complete") return ok();
      const t = setTimeout(ok, 3000);
      pc.onicegatheringstatechange = () => { if (pc.iceGatheringState === "complete") { clearTimeout(t); ok(); } };
    });
    const answer = await new Promise((ok) => {
      answerWait = ok;
      if (!W.relaySendMsg({ type: "files_offer", sdp: JSON.stringify(pc.localDescription) })) {
        answerWait = null;
        ok({ err: "not connected to the computer" });
      }
      setTimeout(() => { if (answerWait === ok) { answerWait = null; ok({ err: "the computer did not answer — its wado may be older than this page" }); } }, 15000);
    });
    if (answer.err) { lost("refused"); throw new Error(answer.err); }
    await pc.setRemoteDescription(JSON.parse(answer.sdp));
    await new Promise((ok, no) => {
      if (dc.readyState === "open") return ok();
      const t = setTimeout(() => no(new Error("no network path to the computer for files (ICE)")), 20000);
      dc.onopen = () => { clearTimeout(t); ok(); };
    });
    F.ready = true;
    F.info = await F.req("hello");
    rlog("open — access " + F.info.access);
    return F.info;
  }

  F.connect = () => {
    if (F.ready) return Promise.resolve(F.info);
    if (!connecting) connecting = open().catch((e) => { connecting = null; lost(String(e)); throw e; });
    return connecting;
  };

  function send(obj) {
    if (!F.dc || F.dc.readyState !== "open") return false;
    F.dc.send(JSON.stringify(obj));
    return true;
  }

  // A request whose first answer settles it.
  F.req = (op, args = {}) => new Promise((ok, no) => {
    const id = nextId++;
    handlers.set(id, { msg: (m) => { handlers.delete(id); if (m.err) no(new Error(m.err)); else ok(m); } });
    if (!send({ id, op, ...args })) { handlers.delete(id); no(new Error("not connected")); }
  });

  // A request with many answers (a transfer). Ends when `h.msg` returns true or the link drops.
  F.stream = (op, args, h) => {
    const id = nextId++;
    handlers.set(id, { msg: (m) => { if (h.msg(m)) handlers.delete(id); }, bytes: h.bytes });
    if (!send({ id, op, ...args })) { handlers.delete(id); h.msg({ err: "not connected", lost: true }); }
    return id;
  };
  F.forget = (id) => handlers.delete(id);

  F.bytes = (id, u8) => {
    const frame = new Uint8Array(4 + u8.length);
    new DataView(frame.buffer).setUint32(0, id);
    frame.set(u8, 4);
    F.dc.send(frame);
  };
  F.buffered = () => (F.dc ? F.dc.bufferedAmount : 0);
  F.open = () => !!(F.dc && F.dc.readyState === "open");
})();
