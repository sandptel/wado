// wado bridge — who is who for the end-to-end envelope (WADO_PLAN.md, Decision Log 2026-10-04).
//
// This device's identity is an Ed25519 key made by WebCrypto as NON-extractable and kept in
// IndexedDB: page code can sign with it, nothing can read it out. Clearing site data makes a new
// key, which this computer sees as a new device — re-pair with its QR code.
//
// Each computer's identity is pinned per Remote ID: from the QR link's `hk` (storage.js writes
// it), else on first sight. A computer that later presents another key is refused outright —
// that is exactly what a relay standing in for it looks like.

W.b64u = {
  enc: (u8) => btoa(String.fromCharCode(...new Uint8Array(u8))).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, ""),
  dec: (s) => Uint8Array.from(atob(String(s).replace(/-/g, "+").replace(/_/g, "/")), (c) => c.charCodeAt(0)),
};

const IDB_NAME = "wado", IDB_STORE = "keys", DEVICE = "device";

function idb(mode, fn) {
  return new Promise((ok, no) => {
    const open = indexedDB.open(IDB_NAME, 1);
    open.onupgradeneeded = () => open.result.createObjectStore(IDB_STORE);
    open.onerror = () => no(open.error);
    open.onsuccess = () => {
      const tx = open.result.transaction(IDB_STORE, mode);
      const req = fn(tx.objectStore(IDB_STORE));
      tx.oncomplete = () => ok(req.result);
      tx.onerror = () => no(tx.error);
    };
  });
}

let devicePromise = null;
// `{ privateKey, publicKey, pub }` — `pub` the raw 32-byte public key.
W.e2eDevice = () => devicePromise || (devicePromise = (async () => {
  let kp = null;
  try { kp = await idb("readonly", (s) => s.get(DEVICE)); } catch (_) {}
  if (!kp) {
    kp = await crypto.subtle.generateKey({ name: "Ed25519" }, false, ["sign", "verify"]);
    // Private mode / no IndexedDB: the key lasts this page load, and the computer sees a
    // new device each time. Works; just asks again.
    try { await idb("readwrite", (s) => s.put(kp, DEVICE)); } catch (_) {}
  }
  const pub = new Uint8Array(await crypto.subtle.exportKey("raw", kp.publicKey));
  return { privateKey: kp.privateKey, publicKey: kp.publicKey, pub };
})().catch((e) => { devicePromise = null; throw e; }));

const PINS = "wado.hostpins";
const normId = (id) => String(id || "").replace(/\D/g, "");
const pins = () => { try { return JSON.parse(localStorage.getItem(PINS)) || {}; } catch (_) { return {}; } };
// `{ pin, from: "qr" | "first-use" }` for a Remote ID, or null.
W.e2eHostPin = (id) => pins()[normId(id)] || null;
W.e2eSetHostPin = (id, pin, from) => {
  try {
    const all = pins();
    all[normId(id)] = { pin, from };
    localStorage.setItem(PINS, JSON.stringify(all));
  } catch (_) {}
};
