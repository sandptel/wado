// wado bridge — light links: a control-only connection to each computer home has a widget for,
// other than the selected one (that one has the full link, relay_link.js). No picture, no sound,
// no session: the sealed relay link alone, asking for the computer's state, its config (what this
// phone may do there), its clipboard and its actions — so a widget shows live values and its
// controls act on the right computer. Decided 2026-10-30; dropped while a desktop session
// streams, because the downlink then belongs to the picture.
//
//   W.sideWatch([[key, relay, remoteId, wants], …])   hold exactly these links; drop the rest.
//                                                     `wants`: "clips" and/or "actions"
//   W.sideDrop(key)          drop one now (it is about to become the selected computer)
//   W.sideDo(key, msg)       send a sealed message on that computer's link
//   W.sideClipCopy(key, id, kind) / W.sidePaste(key)   the Clipboard widget's two taps
//
// **A light link takes no seat** (decided 2026-10-30): it joins with `control=1`, which a relay
// with the `control` cap turns into a seatless control-only link (crates/relay/src/controls.rs) —
// it holds, takes and blocks no device's desktop, and the daemon serves it controls only
// (crates/server/src/control). Before every dial the relay is asked (`/online/:id`) whether it
// and the computer serve such links; an older relay never says so, and is never sent
// `control=1` — it would read it as an ordinary join and give the widget a seat. Then there is
// no link: the widget says what it lacks, and keeps what it last knew.
//
// Each link is its own WebSocket and its own secure handshake — the same wire and transcript as
// e2e_handshake.js (whose `sha256`, `cat`, `lp` and `E2E_LBL` helpers this reuses: the bridge
// files share one script scope), but with its state per link instead of the one global.
//
// To Rust: {type:"side", key, state, host?, config?, clips?, actions?} — fields only when they
// change. `state`: connecting | up | offline | refused | unreachable | old_relay (the relay
// serves no control links) | old_daemon (the computer's wado does not).

(() => {
  const links = new Map(); // key -> link
  const POLL_MS = 3000;
  const say = (l, extra) => emit(Object.assign({ type: "side", key: l.key, state: l.state }, extra || {}));
  const setState = (l, s) => { if (l.state !== s) { l.state = s; say(l); } };
  const rlog = (s) => { if (W.rlog) W.rlog("light link: " + s); };

  function wsUrl(relay, id) {
    const base = String(relay).replace(/^https:\/\//, "wss://").replace(/^http:\/\//, "ws://").replace(/\/+$/, "");
    const norm = String(id).replace(/[\s-]/g, "");
    let inst = "";
    try { inst = (JSON.parse(localStorage.getItem("wado.instance")) || {})[norm] || ""; } catch (_) {}
    return base + "/join/" + encodeURIComponent(norm) + "?client=" + encodeURIComponent(W.clientKey) +
      "&name=" + encodeURIComponent(W.deviceName) + (inst ? "&instance=" + encodeURIComponent(inst) : "") +
      "&control=1";
  }

  // Does this relay, for this computer, serve control-only links? `old_relay` / `old_daemon` /
  // `offline` / `unreachable`, or "" to go ahead.
  async function ask(l) {
    try {
      const url = String(l.relay).replace(/\/+$/, "") + "/online/" + encodeURIComponent(String(l.id).replace(/[\s-]/g, ""));
      const j = await (await fetch(url, { cache: "no-store" })).json();
      if (!(j.daemons > 0)) return "offline";
      if (typeof j.control !== "number") return "old_relay";
      if (!(j.control > 0)) return "old_daemon";
      return "";
    } catch (_) {
      return "unreachable";
    }
  }

  // Ask first, then dial — or wait and ask again. A relay or a computer that cannot serve a
  // control link is asked again only now and then: an update is not something to poll for.
  async function dial(l) {
    if (l.dead) return;
    clearTimeout(l.retry);
    const why = await ask(l);
    if (l.dead) return;
    if (!why) return open(l);
    setState(l, why);
    l.retry = setTimeout(() => dial(l), why === "old_relay" || why === "old_daemon" ? 120000 : why === "offline" ? 15000 : 20000);
  }

  function send(l, obj) {
    if (!l.chan || l.e2e !== "open" || !l.ws || l.ws.readyState !== WebSocket.OPEN) return false;
    l.chan.send(JSON.stringify(obj), l.ws);
    return true;
  }

  function open(l) {
    if (l.dead) return;
    clearTimeout(l.retry);
    let ws;
    try { ws = new WebSocket(wsUrl(l.relay, l.id)); } catch (_) { return later(l, "unreachable"); }
    l.ws = ws; l.e2e = ""; l.chan = null;
    if (l.state !== "up") setState(l, "connecting");
    ws.onmessage = (ev) => {
      let m;
      try { m = JSON.parse(ev.data); } catch (_) { return; }
      if (m.type === "ping") { try { ws.send(JSON.stringify({ type: "pong" })); } catch (_) {} return; }
      if (m.type === "waiting") { setState(l, "offline"); return; }
      if (m.type === "join_denied") {
        setState(l, /older than live widgets/i.test(m.reason || "") ? "old_daemon" : "offline");
        l.backoff = Math.max(l.backoff, 15000);
        return;
      }
      // Belt and braces: a relay that answered with a seat did not make a control link. Leave at
      // once (4001 frees the seat) rather than hold another device's desktop.
      if (m.type === "join_accepted" && m.assignment !== "control") {
        l.refused = true;
        setState(l, "old_relay");
        try { ws.close(4001, "leave"); } catch (_) {}
        return;
      }
      if (m.type === "join_accepted") { hello(l, m.room_id || ""); return; }
      if (m.type === "e2e_reply") { reply(l, m); return; }
      if (m.type === "e2e_fail") { l.refused = true; setState(l, "refused"); try { ws.close(4001, "leave"); } catch (_) {} return; }
      if (m.type === "sealed" && l.chan) {
        l.chan.recv(m, (text) => onSealed(l, JSON.parse(text)), () => { try { ws.close(); } catch (_) {} });
      }
    };
    ws.onerror = () => {};
    ws.onclose = () => {
      if (l.ws !== ws) return;
      l.ws = null; l.chan = null; l.e2e = "";
      clearInterval(l.poll);
      if (l.dead) return;
      if (l.refused) return;
      later(l, l.state === "up" ? "connecting" : l.state === "connecting" ? "unreachable" : l.state);
    };
  }

  function later(l, state) {
    if (l.dead) return;
    setState(l, state);
    l.backoff = Math.min(Math.max(l.backoff * 2, 2000), 30000);
    l.retry = setTimeout(() => dial(l), l.backoff);
  }

  // The handshake, per link (e2e_handshake.js has the reasoning; this is the same transcript).
  async function hello(l, room) {
    l.room = String(room);
    try {
      l.eph = await crypto.subtle.generateKey({ name: "X25519" }, false, ["deriveBits"]);
      l.ephPub = new Uint8Array(await crypto.subtle.exportKey("raw", l.eph.publicKey));
      await W.e2eDevice();
    } catch (_) { l.refused = true; return setState(l, "refused"); }
    l.e2e = "hello";
    try { l.ws.send(JSON.stringify({ type: "e2e_hello", v: 1, eph: W.b64u.enc(l.ephPub) })); } catch (_) {}
  }

  async function reply(l, msg) {
    if (l.e2e !== "hello") return;
    l.e2e = "finish";
    try {
      const ephD = W.b64u.dec(msg.eph), hpk = W.b64u.dec(msg.host_pk);
      const pin = W.b64u.enc(await sha256(hpk));
      const known = W.e2eHostPin(l.id);
      // A computer that changed identity is refused here exactly as on the main link.
      if (known && known.pin !== pin) { l.refused = true; rlog(l.id + ": identity changed — refused"); return setState(l, "refused"); }
      const t1 = cat(E2E_LBL, lp(l.room), lp(W.clientKey), l.ephPub, ephD, hpk);
      const h1 = await sha256(t1);
      const hostKey = await crypto.subtle.importKey("raw", hpk, { name: "Ed25519" }, false, ["verify"]);
      if (!(await crypto.subtle.verify({ name: "Ed25519" }, hostKey, W.b64u.dec(msg.sig), h1))) {
        l.refused = true;
        return setState(l, "refused");
      }
      if (!known) W.e2eSetHostPin(l.id, pin, "first-use");
      const peer = await crypto.subtle.importKey("raw", ephD, { name: "X25519" }, false, []);
      const shared = await crypto.subtle.deriveBits({ name: "X25519", public: peer }, l.eph.privateKey, 256);
      const keys = await W.e2eDerive(shared, h1);
      const dev = await W.e2eDevice();
      const h2 = await sha256(cat(t1, dev.pub));
      const sig = await crypto.subtle.sign({ name: "Ed25519" }, dev.privateKey, h2);
      l.chan = W.e2eChannel(keys);
      l.e2e = "sealed";
      // No pairing code ever rides a light link: the code belongs to the link a person opened.
      l.ws.send(JSON.stringify({ type: "e2e_finish", dev_pk: W.b64u.enc(dev.pub), sig: W.b64u.enc(sig), pair_mac: "" }));
    } catch (_) {
      try { l.ws.close(); } catch (_) {}
    }
  }

  function onSealed(l, m) {
    if (l.e2e !== "open") {
      if (m.type !== "e2e_ok") return;
      l.e2e = "open";
      l.backoff = 0;
      setState(l, "up");
      askAll(l);
      clearInterval(l.poll);
      let tick = 0;
      l.poll = setInterval(() => {
        send(l, { type: "host_get" });
        // Nothing is pushed on a control link, so the clipboard is asked for now and then.
        if (l.wants.includes("clips") && ++tick % 4 === 0) send(l, { type: "clip_list" });
      }, POLL_MS);
      return;
    }
    switch (m.type) {
      case "host_state": {
        const st = m.state || null;
        for (const p of (st && st.media) || []) {
          const had = l.pics[p.bus] || {};
          if (p.art_same) p.art = had.art || null;
          if (p.icon_same) p.icon = had.icon || null;
          l.pics[p.bus] = { art: p.art, icon: p.icon };
        }
        say(l, { host: st });
        break;
      }
      case "config_state": say(l, { config: m.state || null }); break;
      case "clip_history": say(l, { clips: m.entries || [] }); break;
      case "clip_data":
        l.clipFull[m.id] = { mime: m.mime, data: m.data };
        (l.clipWait[m.id] || []).forEach((f) => f(l.clipFull[m.id]));
        delete l.clipWait[m.id];
        break;
      case "pad_out":
        if (m.msg && m.msg.k === "action_list") say(l, { actions: m.msg.actions || [] });
        if (m.msg && m.msg.k === "ran" && !m.msg.ok) emit({ type: "ran", message: m.msg.message || "That did not work." });
        break;
      case "host_error": emit({ type: "captureNote", text: m.message || "That did not work." }); break;
      default: break;
    }
  }

  // What a fresh link asks for: the state always, the rest when a widget wants it.
  function askAll(l) {
    send(l, { type: "host_get" });
    send(l, { type: "config_get" });
    if (l.wants.includes("clips")) send(l, { type: "clip_list" });
    if (l.wants.includes("actions")) send(l, { type: "pad_in", msg: { k: "actions" } });
  }

  function drop(key) {
    const l = links.get(key);
    if (!l) return;
    links.delete(key);
    l.dead = true;
    clearTimeout(l.retry);
    clearInterval(l.poll);
    // 4001 = leave: the relay frees the seat rather than holding it for us.
    if (l.ws) { const ws = l.ws; l.ws = null; ws.onclose = null; try { ws.close(4001, "leave"); } catch (_) {} }
  }

  W.sideWatch = (list) => {
    const want = new Map((list || []).map(([key, relay, id, wants]) => [key, { relay, id, wants: wants || [] }]));
    for (const key of [...links.keys()]) {
      const w = want.get(key), l = links.get(key);
      if (!w || w.relay !== l.relay || w.id !== l.id) drop(key);
    }
    for (const [key, w] of want) {
      const l = links.get(key);
      if (l) {
        const more = w.wants.filter((x) => !l.wants.includes(x));
        l.wants = w.wants;
        if (more.length && l.e2e === "open") askAll(l);
        // A link given up on (refused) is tried again on a fresh watch.
        if (l.refused && !l.ws) { l.refused = false; dial(l); }
        continue;
      }
      const nl = { key, relay: w.relay, id: w.id, wants: w.wants, state: "", ws: null, chan: null, e2e: "", backoff: 0,
        pics: {}, clipFull: {}, clipWait: {}, retry: 0, poll: 0, dead: false, refused: false };
      links.set(key, nl);
      dial(nl);
    }
  };
  W.sideDrop = drop;
  W.sideDo = (key, msg) => { const l = links.get(key); return !!(l && send(l, msg)); };

  // Inside a tap, as cliphost.js does for the selected computer: the write is the tap's, however
  // long the fetch takes.
  W.sideClipCopy = (key, id, kind) => {
    const l = links.get(key);
    const note = (text) => emit({ type: "captureNote", text });
    if (!l || !navigator.clipboard || !window.ClipboardItem) return note("This browser has no clipboard to write to.");
    const image = kind === "image";
    const got = l.clipFull[id] ? Promise.resolve(l.clipFull[id]) : new Promise((ok, no) => {
      (l.clipWait[id] = l.clipWait[id] || []).push(ok);
      send(l, { type: "clip_get", id });
      setTimeout(() => no(new Error("no answer")), 8000);
    });
    const blob = got.then(async (c) => image ? await (await fetch(c.data)).blob() : new Blob([c.data], { type: "text/plain" }));
    navigator.clipboard.write([new ClipboardItem({ [image ? "image/png" : "text/plain"]: blob })])
      .then(() => note("Copied to this device."), () => note("This browser would not take it — tap again."));
  };
  W.sidePaste = async (key) => {
    const note = (text) => emit({ type: "captureNote", text });
    try {
      const t = await navigator.clipboard.readText();
      if (!t) return note("This device's clipboard is empty.");
      if (W.sideDo(key, { type: "clip_push", mime: "text/plain", data: t })) note("Sent to the computer's clipboard.");
    } catch (_) { note("This browser would not share its clipboard."); }
  };
})();
