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
//   W.files.lane                     → true while it rides the relay's limited lane
//   W.files.retryDirect()            → drop the lane and try for a direct path again
//
// **No direct path** (ICE fails — a VPN, two carrier NATs): the same requests go sealed through
// the relay link instead (`files_in` / `files_out` / `files_chunk`), the fallback lane — 64 KB/s,
// listings and files up to 1 MB; thumbnails, archives, playback and bigger files are refused by
// the computer with `lane: true` and wait for Direct (Decision Log 2026-10-27). Also taken at
// once when the session itself is already relayed (`W.linkNow.state`): no point waiting out ICE.

(() => {
  const F = (W.files = W.files || {});
  F.pc = null;
  F.dc = null;
  F.ready = false;
  F.lane = false;
  F.info = null; // hello: { roots, home, access, device }
  let nextId = 1;
  let connecting = null;
  let answerWait = null;
  const handlers = new Map(); // id → { msg, bytes }
  const downs = [];
  // How long a direct attempt may take to open before files fall back to the lane.
  const DIRECT_WAIT_MS = 8000;

  const rlog = (s) => { if (W.rlog) W.rlog("files: " + s); };

  F.onDown = (fn) => downs.push(fn);

  function lost(why) {
    if (!F.pc && !F.ready) return;
    rlog("link lost — " + why);
    F.ready = false;
    F.lane = false;
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
  // its own path, but nothing new can be negotiated until the link is back. On the lane, the
  // link *is* the path: it is lost with it.
  {
    const prev = W._relayHandlers["__down"];
    W.relayOn("__down", (m) => {
      if (prev) prev(m);
      if (answerWait) { const w = answerWait; answerWait = null; w({ err: "the link to the computer dropped" }); }
      if (F.lane) lost("the relay link dropped");
    });
  }

  // One answer, from either path. `lane: true` is the computer saying the lane cannot carry it.
  function answer(m) {
    if (m.lane && W.laneHint) W.laneHint("files", m.err || "");
    const h = handlers.get(m.id);
    if (h) h.msg(m);
  }
  W.relayOn("files_out", (m) => { if (!F.lane) return; try { answer(JSON.parse(m.text)); } catch (_) {} });
  W.relayOn("files_chunk", (m) => {
    if (!F.lane) return;
    const h = handlers.get(m.id);
    if (!h || !h.bytes) return;
    const s = atob(m.b || "");
    const u8 = new Uint8Array(s.length);
    for (let i = 0; i < s.length; i++) u8[i] = s.charCodeAt(i);
    h.bytes(u8);
  });

  async function openLane(why) {
    rlog("on the relay's limited lane — " + why);
    F.lane = true;
    F.ready = true;
    try {
      // A computer from before the lane does not answer `files_in` at all.
      F.info = await Promise.race([F.req("hello"), new Promise((_, no) => setTimeout(() => no(new Error(
        "no direct path to the computer for files, and its wado is older than files on the relay — update it there")), 8000))]);
    } catch (e) {
      F.lane = false;
      F.ready = false;
      throw e;
    }
    if (W.laneHint) W.laneHint("files", "");
    rlog("open on the lane — access " + F.info.access);
    return F.info;
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
        answer(m);
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
    const reply = await new Promise((ok) => {
      answerWait = ok;
      if (!W.relaySendMsg({ type: "files_offer", sdp: JSON.stringify(pc.localDescription) })) {
        answerWait = null;
        ok({ err: "not connected to the computer" });
      }
      setTimeout(() => { if (answerWait === ok) { answerWait = null; ok({ err: "the computer did not answer — its wado may be older than this page" }); } }, 15000);
    });
    if (reply.err) { lost("refused"); throw new Error(reply.err); }
    await pc.setRemoteDescription(JSON.parse(reply.sdp));
    const direct = await new Promise((ok) => {
      if (dc.readyState === "open") return ok(true);
      const t = setTimeout(() => ok(false), DIRECT_WAIT_MS);
      dc.onopen = () => { clearTimeout(t); ok(true); };
    });
    if (!direct) {
      // No direct path: the lane carries what it can, rather than nothing at all.
      try { pc.close(); } catch (_) {}
      if (F.pc === pc) { F.pc = null; F.dc = null; }
      return openLane("no direct path (ICE did not connect in " + DIRECT_WAIT_MS / 1000 + " s)");
    }
    F.ready = true;
    F.info = await F.req("hello");
    if (W.laneHintClear) W.laneHintClear("files");
    rlog("open — access " + F.info.access);
    return F.info;
  }

  // The session already found no direct path: do not wait out ICE again for files.
  const sessionRelayed = () => !!(W.linkNow && W.linkNow.state === "relayed");

  F.connect = () => {
    if (F.ready) return Promise.resolve(F.info);
    if (!connecting) {
      const go = sessionRelayed() ? openLane("the session has no direct path") : open();
      connecting = go.catch((e) => { connecting = null; lost(String(e)); throw e; });
    }
    return connecting;
  };

  F.retryDirect = () => {
    if (F.lane) lost("retrying direct");
    connecting = null;
    connecting = open().catch((e) => { connecting = null; lost(String(e)); throw e; });
    return connecting;
  };

  function send(obj) {
    if (F.lane) return !!(W.relayUp && W.relaySendMsg({ type: "files_in", text: JSON.stringify(obj) }));
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
    if (F.lane) {
      let s = "";
      for (let i = 0; i < u8.length; i += 0x8000) s += String.fromCharCode.apply(null, u8.subarray(i, i + 0x8000));
      W.relaySendMsg({ type: "files_chunk", id, b: btoa(s) });
      return;
    }
    const frame = new Uint8Array(4 + u8.length);
    new DataView(frame.buffer).setUint32(0, id);
    frame.set(u8, 4);
    F.dc.send(frame);
  };
  // On the lane, what the relay socket has not yet sent: the relay holds it to 64 KB/s, so this
  // is what keeps an upload from queueing a megabyte in the browser.
  F.buffered = () => (F.lane ? (W.relayWs ? W.relayWs.bufferedAmount : 0) : F.dc ? F.dc.bufferedAmount : 0);
  F.open = () => (F.lane ? !!W.relayUp : !!(F.dc && F.dc.readyState === "open"));
})();
