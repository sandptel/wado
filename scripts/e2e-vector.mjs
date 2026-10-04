// The envelope's shared test vector, computed with WebCrypto — the API the browser client uses
// (js/e2e.js). Must print what crates/server/src/e2e/seal.rs `vector` pins.
//
//   node scripts/e2e-vector.mjs
const { subtle } = globalThis.crypto;
const hex = (b) => Buffer.from(b).toString("hex");
const enc = new TextEncoder();
const ikm = await subtle.importKey("raw", new Uint8Array(32).fill(7), "HKDF", false, ["deriveBits"]);
const okm = new Uint8Array(await subtle.deriveBits(
  { name: "HKDF", hash: "SHA-256", salt: new Uint8Array(32).fill(9), info: enc.encode("wado-e2e-v1") }, ikm, 512));
const c2d = okm.slice(0, 32), d2c = okm.slice(32);
const k = await subtle.importKey("raw", c2d, "AES-GCM", false, ["encrypt"]);
const ct = await subtle.encrypt({ name: "AES-GCM", iv: new Uint8Array(12) }, k, enc.encode('{"type":"session_stop"}'));
const want = {
  c2d: "3333fc281be52212d97b855583beacebcc8775e996ca1ce69d1dc46f6d778255",
  d2c: "37a538fc09ba3027b0137fcdd446183998eba633f241367567d8a56b0b34f7e2",
  c: "7fa035c9163e20c5ff7922a95b3651312a584ad5c306d45a69d49b882db0b85c875a56044a9be2",
};
const got = { c2d: hex(c2d), d2c: hex(d2c), c: hex(ct) };
for (const f of Object.keys(want)) if (got[f] !== want[f]) { console.error("MISMATCH", f, got[f]); process.exit(1); }
console.log("e2e vector: WebCrypto matches the daemon");
