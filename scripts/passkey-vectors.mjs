// Make the daemon's WebAuthn test vectors once, with WebCrypto (an implementation that is not
// the daemon's), into crates/server/src/passkey/vectors.json.
//
//   node scripts/passkey-vectors.mjs
//
// The vectors are committed. Rerunning makes a new key and new bytes, and the tests that read
// them keep passing: they pin only the shapes and the challenges/origin below.
import { mkdirSync, writeFileSync } from "node:fs";
import { authenticator, b64u } from "./lib/e2e-passkey.mjs";

const ORIGIN = "https://wado.example";
const C_REG = b64u(new Uint8Array(32).fill(7));
const C_GET = b64u(new Uint8Array(32).fill(9));
const pk = await authenticator();
const reg = await pk.register(C_REG, ORIGIN);
pk.counter = 4; // the next assertion counts 5
const get = await pk.assert(C_GET, ORIGIN);
const synced = await authenticator();
const sreg = await synced.register(C_REG, ORIGIN);
const sget = await synced.assert(C_GET, ORIGIN); // counter stays 0, as a synced passkey's does
const out = {
  origin: ORIGIN,
  rp_id: new URL(ORIGIN).hostname,
  register_challenge: C_REG,
  get_challenge: C_GET,
  register: reg,
  assert: get,
  synced_register: sreg,
  synced_assert: sget,
};
const dir = new URL("../crates/server/src/passkey/", import.meta.url);
mkdirSync(dir, { recursive: true });
writeFileSync(new URL("vectors.json", dir), JSON.stringify(out, null, 2) + "\n");
console.log("wrote", new URL("vectors.json", dir).pathname);
