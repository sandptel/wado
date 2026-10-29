// The envelope's shared test vector, computed with WebCrypto — the API the browser client uses
// (js/e2e_seal.js). Must print what crates/server/src/e2e/seal.rs `vector`, `pair_vector` and
// `sas_vector` pin.
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
// The typed pairing code's key and proof: k_pair from the same secret and salt, then
// HMAC(k_pair, H(T2) ‖ code) with H(T2) = 32 bytes of 5 and the code "K7QF2MXP".
const kPair = new Uint8Array(await subtle.deriveBits(
  { name: "HKDF", hash: "SHA-256", salt: new Uint8Array(32).fill(9), info: enc.encode("wado-e2e-v1 pair") }, ikm, 256));
const hk = await subtle.importKey("raw", kPair, { name: "HMAC", hash: "SHA-256" }, false, ["sign"]);
const msg = new Uint8Array([...new Uint8Array(32).fill(5), ...enc.encode("K7QF2MXP")]);
const typedMac = new Uint8Array(await subtle.sign("HMAC", hk, msg));
// The check words: k_sas from the same secret and salt, then HMAC(k_sas, H(T2) ‖ nonce)[..4]
// with the nonce 32 bytes of 6, read through the same word list the client bundles.
const { readFileSync } = await import("node:fs");
const WORDS = readFileSync(new URL("../crates/protocol/src/sas_words.txt", import.meta.url), "utf8").trim().split(/\s+/);
const kSas = new Uint8Array(await subtle.deriveBits(
  { name: "HKDF", hash: "SHA-256", salt: new Uint8Array(32).fill(9), info: enc.encode("wado-e2e-v1 sas") }, ikm, 256));
const sk = await subtle.importKey("raw", kSas, { name: "HMAC", hash: "SHA-256" }, false, ["sign"]);
const sas = new Uint8Array(await subtle.sign("HMAC", sk, new Uint8Array([...new Uint8Array(32).fill(5), ...new Uint8Array(32).fill(6)]))).slice(0, 4);
const want = {
  c2d: "3333fc281be52212d97b855583beacebcc8775e996ca1ce69d1dc46f6d778255",
  d2c: "37a538fc09ba3027b0137fcdd446183998eba633f241367567d8a56b0b34f7e2",
  c: "7fa035c9163e20c5ff7922a95b3651312a584ad5c306d45a69d49b882db0b85c875a56044a9be2",
  kPair: "a9b2a7e836c7ae6a4ae6b4fa35d020280d1ba01f03c2c6e93753e4120fc6a18c",
  typedMac: "b1ae135713327fcf863f9cb8d7f8cfdc26c6fd99fb7c72cda383e7fcfe207447",
  kSas: "fcd6888ab08b88c8073ffd51b268af29878f82fef676f19bd1c0a7b1e3f0b51e",
  sas: "desert badger torch shell",
};
const got = { c2d: hex(c2d), d2c: hex(d2c), c: hex(ct), kPair: hex(kPair), typedMac: hex(typedMac),
  kSas: hex(kSas), sas: [...sas].map((b) => WORDS[b]).join(" ") };
for (const f of Object.keys(want)) if (got[f] !== want[f]) { console.error("MISMATCH", f, got[f]); process.exitCode = 1; }
if (!process.exitCode) console.log("e2e vector: WebCrypto matches the daemon");
