// wado bridge — the envelope's keys and sealing. AES-256-GCM per direction, a strict counter
// each way; the daemon's half is crates/server/src/e2e/seal.rs and scripts/e2e-vector.mjs pins
// that both compute the same bytes.
//
// WebCrypto is async and does not promise to finish operations in the order they started, so
// both directions run through a promise chain: frames leave, and are handled, in counter order.

const E2E_LABEL = new TextEncoder().encode("wado-e2e-v1");

// `{ c2d, d2c }` AES-GCM keys from the X25519 shared secret and H(T1).
W.e2eDerive = async (shared, salt) => {
  const ikm = await crypto.subtle.importKey("raw", shared, "HKDF", false, ["deriveBits"]);
  const okm = new Uint8Array(await crypto.subtle.deriveBits(
    { name: "HKDF", hash: "SHA-256", salt, info: E2E_LABEL }, ikm, 512));
  const key = (b, use) => crypto.subtle.importKey("raw", b, "AES-GCM", false, [use]);
  return { c2d: await key(okm.slice(0, 32), "encrypt"), d2c: await key(okm.slice(32), "decrypt") };
};

const nonceOf = (n) => {
  const iv = new Uint8Array(12);
  new DataView(iv.buffer).setBigUint64(4, BigInt(n));
  return iv;
};

// One sealed link. `send(text, ws)` seals and sends in order; `recv(frame, deliver, fail)`
// opens in order and hands each plaintext to `deliver`, or calls `fail` once and goes dead.
W.e2eChannel = (keys) => {
  let outN = 0, inN = 0, outQ = Promise.resolve(), inQ = Promise.resolve(), dead = false, pending = 0;
  const enc = new TextEncoder(), dec = new TextDecoder();
  return {
    // Nothing sealed is still waiting to be sent.
    idle: () => pending === 0,
    // Settles once every frame sealed so far has been handed to the socket.
    flushed: () => outQ,
    send(text, ws) {
      const n = outN++;
      pending++;
      outQ = outQ.then(async () => {
        const c = await crypto.subtle.encrypt({ name: "AES-GCM", iv: nonceOf(n) }, keys.c2d, enc.encode(text));
        if (!dead && ws.readyState === WebSocket.OPEN) ws.send(JSON.stringify({ type: "sealed", n, c: W.b64u.enc(c) }));
      }).catch(() => {}).finally(() => { pending--; });
    },
    recv(msg, deliver, fail) {
      inQ = inQ.then(async () => {
        if (dead) return;
        let p;
        try {
          if (msg.n !== inN) throw new Error("out of order");
          p = await crypto.subtle.decrypt({ name: "AES-GCM", iv: nonceOf(msg.n) }, keys.d2c, W.b64u.dec(msg.c));
          inN++;
        } catch (e) {
          dead = true;
          fail(e);
          return;
        }
        // Outside the try: a handler that throws is a handler bug, not a broken link.
        try { deliver(dec.decode(p)); } catch (e) { if (W.rlog) W.rlog("e2e: handler threw: " + e); }
      });
    },
  };
};
