// A browser device for the e2e scripts: the client's own envelope code (crates/client/src/js/
// e2e_*.js), run unchanged in a sandbox with its own storage — so the harness tests what ships,
// not a re-implementation of it.
//
//   const phone = device();                 one browser: one device key, one set of host pins
//   phone.store.setItem("wado.pair", code)  as the QR link's storage.js would
//   link(ws, phone, msgs)                   speak the envelope on a relay socket
import { readFileSync } from "node:fs";
import vm from "node:vm";

const JS = ["e2e_keys", "e2e_seal", "e2e_handshake"]
  .map((f) => readFileSync(new URL(`../../crates/client/src/js/${f}.js`, import.meta.url), "utf8"))
  .join("\n");

export function device(clientKey = "") {
  const m = new Map();
  const store = {
    getItem: (k) => (m.has(k) ? m.get(k) : null),
    setItem: (k, v) => m.set(k, String(v)),
    removeItem: (k) => m.delete(k),
  };
  const W = { clientKey, log: [] };
  W.rlog = (l) => W.log.push(l);
  // No indexedDB here: the key lives as long as this object, i.e. one "browser".
  const ctx = vm.createContext({
    W, crypto, atob, btoa, TextEncoder, TextDecoder, WebSocket, BigInt, DataView,
    Uint8Array, setTimeout, clearTimeout, localStorage: store, status: () => {}, console,
  });
  vm.runInContext(JS, ctx);
  return { W, store };
}

// Handle one relay frame the way relay_link.js does. Returns true if it was the envelope's.
export function onFrame(dev, ws, remoteId, m, push) {
  const W = dev.W;
  if (m.type === "join_accepted") {
    push(m);
    W.e2eBegin(ws, m.room_id, remoteId, () => push({ type: "__up" }));
    return true;
  }
  if (m.type === "e2e_reply") { W.e2eOnReply(m); return true; }
  if (m.type === "e2e_fail") { push(m); W.e2eOnFail(m); return true; }
  if (m.type === "sealed") { W.e2eOnSealed(m, push); return true; }
  return false;
}

// One browser per client key, shared by every connection the script makes with that key.
const browsers = {};
export const deviceFor = (key) => (browsers[key] ||= device(key));

// Wire a relay socket for a harness client: pongs, the envelope, and `send` that waits for the
// secure link (as the real client's senders wait for `relayUp`). `push` gets every frame the
// harness should see — relay frames as they are, the computer's opened.
export function secure(ws, dev, remoteId, push) {
  const queue = [];
  ws.addEventListener("message", (e) => {
    const m = JSON.parse(e.data);
    if (m.type === "ping") return ws.send(JSON.stringify({ type: "pong" }));
    const out = (x) => {
      if (x.type === "__up") queue.splice(0).forEach((q) => dev.W.e2eSend(q));
      push(x);
    };
    if (!onFrame(dev, ws, remoteId, m, out)) push(m);
  });
  return (m) => { if (!dev.W.e2eSend(m)) queue.push(m); };
}
