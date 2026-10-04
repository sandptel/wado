// wado bridge — the envelope's handshake, client half (wire: crates/protocol/src/envelope.rs).
//
//   join_accepted → e2e_hello{eph} → e2e_reply{eph, host_pk, sig} → check the pin and the
//   signature → e2e_finish{dev_pk, sig, pair_mac} → sealed e2e_ok → the link is up.
//
// Until e2e_ok nothing is sent to the computer, and nothing it did not seal is acted on: the relay
// can neither read nor forge a message. The QR pairing code never leaves this device — it is
// proven with an HMAC over the transcript.

const E2E_REPLY_WAIT_MS = 8000;
const E2E_LBL = new TextEncoder().encode("wado-e2e-v1");

const sha256 = async (b) => new Uint8Array(await crypto.subtle.digest("SHA-256", b));
const cat = (...parts) => {
  const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
  let i = 0;
  for (const p of parts) { out.set(p, i); i += p.length; }
  return out;
};
const lp = (s) => {
  const b = new TextEncoder().encode(s);
  return cat(new Uint8Array([b.length >> 8, b.length & 255]), b);
};

W._e2e = null;
W.e2eReady = false;
// The computer's key was pinned on first use rather than from its QR code (F1 needs a QR pin).
W.e2eUnpinned = false;

function e2eStop(s, why, block) {
  if (W._e2e !== s) return;
  W._e2e = null;
  W.e2eReady = false;
  clearTimeout(s.timer);
  if (block) W._e2eBlocked = true;
  if (W.rlog) W.rlog("secure link: " + why);
  if (W.relayPhase) W.relayPhase(1, why);
  status(why);
  // Refused for good: give the seat back (4001 = leave). Broken: a plain close keeps it for the
  // redial.
  try { if (block) s.ws.close(4001, "leave"); else s.ws.close(); } catch (_) {}
}

// Called on `join_accepted`. `onUp` runs once the computer has let this device in.
W.e2eBegin = async (ws, room, remoteId, onUp) => {
  const s = { ws, room: String(room || ""), id: remoteId, onUp, state: "hello" };
  W._e2e = s;
  W.e2eReady = false;
  try {
    s.eph = await crypto.subtle.generateKey({ name: "X25519" }, false, ["deriveBits"]);
    s.ephPub = new Uint8Array(await crypto.subtle.exportKey("raw", s.eph.publicKey));
    await W.e2eDevice();
  } catch (e) {
    return e2eStop(s, "this browser cannot do wado's secure handshake (needs Chrome 137+, Firefox 130+ or Safari 17+) — " + e, true);
  }
  if (W._e2e !== s) return;
  ws.send(JSON.stringify({ type: "e2e_hello", v: 1, eph: W.b64u.enc(s.ephPub) }));
  s.timer = setTimeout(() => {
    if (W._e2e === s && s.state === "hello")
      e2eStop(s, "this computer did not answer the secure handshake — its wado is older than this page; update it", false);
  }, E2E_REPLY_WAIT_MS);
};

W.e2eOnReply = async (msg) => {
  const s = W._e2e;
  if (!s || s.state !== "hello") return;
  s.state = "finish";
  clearTimeout(s.timer);
  try {
    const ephD = W.b64u.dec(msg.eph), hpk = W.b64u.dec(msg.host_pk);
    const pin = W.b64u.enc(await sha256(hpk));
    const known = W.e2eHostPin(s.id);
    if (known && known.pin !== pin)
      return e2eStop(s, "REFUSED: this computer's identity has changed. Something between you and it " +
        "(the relay or the network) may be impersonating it. If you reinstalled wado there, scan its QR code again.", true);
    const t1 = cat(E2E_LBL, lp(s.room), lp(W.clientKey), s.ephPub, ephD, hpk);
    const h1 = await sha256(t1);
    const hostKey = await crypto.subtle.importKey("raw", hpk, { name: "Ed25519" }, false, ["verify"]);
    if (!(await crypto.subtle.verify({ name: "Ed25519" }, hostKey, W.b64u.dec(msg.sig), h1)))
      return e2eStop(s, "REFUSED: the computer's handshake signature does not verify", true);
    if (!known) W.e2eSetHostPin(s.id, pin, "first-use");
    W.e2eUnpinned = !known || known.from !== "qr";

    const peer = await crypto.subtle.importKey("raw", ephD, { name: "X25519" }, false, []);
    const shared = await crypto.subtle.deriveBits({ name: "X25519", public: peer }, s.eph.privateKey, 256);
    const keys = await W.e2eDerive(shared, h1);
    const dev = await W.e2eDevice();
    const h2 = await sha256(cat(t1, dev.pub));
    const sig = await crypto.subtle.sign({ name: "Ed25519" }, dev.privateKey, h2);
    let pairMac = "";
    const code = (() => { try { return localStorage.getItem("wado.pair") || ""; } catch (_) { return ""; } })();
    if (code) {
      const k = await crypto.subtle.importKey("raw", new TextEncoder().encode(code), { name: "HMAC", hash: "SHA-256" }, false, ["sign"]);
      pairMac = W.b64u.enc(await crypto.subtle.sign("HMAC", k, h2));
    }
    if (W._e2e !== s) return;
    s.chan = W.e2eChannel(keys);
    s.state = "sealed";
    s.ws.send(JSON.stringify({ type: "e2e_finish", dev_pk: W.b64u.enc(dev.pub), sig: W.b64u.enc(sig), pair_mac: pairMac }));
  } catch (e) {
    e2eStop(s, "the secure handshake failed — " + e, false);
  }
};

// Plaintext, so unauthenticated: shown, and the link stops redialling, but nothing else.
W.e2eOnFail = (msg) => {
  if (W._e2e) e2eStop(W._e2e, "this computer refused the device — " + (msg.reason || "no reason given"), true);
};

// A sealed frame from the computer. `dispatch(obj)` gets each opened message, in order.
W.e2eOnSealed = (msg, dispatch) => {
  const s = W._e2e;
  if (!s || !s.chan) return;
  s.chan.recv(msg, (text) => {
    const obj = JSON.parse(text);
    if (s.state !== "open") {
      if (obj.type !== "e2e_ok") return;
      s.state = "open";
      W.e2eReady = true;
      // Proven (or not needed): either way the code has done its job.
      try { localStorage.removeItem("wado.pair"); } catch (_) {}
      if (W.rlog) W.rlog("secure link up — end-to-end sealed" + (W.e2eUnpinned ? " (computer pinned on first use, not by QR)" : ""));
      s.onUp();
      return;
    }
    dispatch(obj);
  }, (e) => e2eStop(s, "the secure link broke (a message was altered or lost) — reconnecting: " + e, false));
};

// Close `ws` once every sealed frame queued for it has left: sealing is async, and a send
// followed by a close must not lose the send (a "leave" is exactly that).
//
// Synchronous when nothing is queued, and that matters: a redial follows a drop at once, and
// the relay refuses a second socket for the same device while the first is still open (the
// "second tab" guard) — a close deferred even by a tick lost that race in pool-e2e.
W.e2eClose = (ws, code, reason) => {
  const close = () => { try { ws.close(code, reason); } catch (_) {} };
  const s = W._e2e;
  if (s && s.ws === ws && s.chan && !s.chan.idle()) s.chan.flushed().then(close);
  else close();
};

// Send one message to the computer, sealed. False until the link is up.
W.e2eSend = (obj) => {
  const s = W._e2e;
  if (!s || s.state !== "open" || s.ws.readyState !== WebSocket.OPEN) return false;
  s.chan.send(JSON.stringify(obj), s.ws);
  return true;
};
