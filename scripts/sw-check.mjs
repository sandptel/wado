// The installed app's release check (crates/client/public/sw-verify.js), run the way the service
// worker runs it — WebCrypto Ed25519 and SHA-256 — against throwaway keys made here.
//
//   node scripts/sw-check.mjs
import { readFileSync } from "node:fs";

const { subtle } = globalThis.crypto;
await import(new URL("../crates/client/public/sw-verify.js", import.meta.url));
const R = globalThis.WadoRelease;
const enc = new TextEncoder();
const b64 = (buf) => Buffer.from(buf).toString("base64");

let failed = 0;
const ok = (cond, what) => { console.log((cond ? "ok   " : "FAIL ") + what); if (!cond) failed++; };

async function keypair() {
  const k = await subtle.generateKey({ name: "Ed25519" }, true, ["sign", "verify"]);
  return { priv: k.privateKey, pub: b64(await subtle.exportKey("raw", k.publicKey)) };
}
const signB64 = async (priv, bytes) => b64(await subtle.sign({ name: "Ed25519" }, priv, bytes));

const files = { "index.html": enc.encode("<!doctype html><title>wado</title>"), "assets/app.js": enc.encode("console.log(1)") };
const hashes = {};
for (const [p, b] of Object.entries(files)) hashes[p] = await R.sha256hex(subtle, b);
const release = (extra = {}) => enc.encode(JSON.stringify({ app: "wado", version: "0.0.4", commit: "abc", built: "2026-10-30T10:00:00Z", files: hashes, ...extra }));

const good = await keypair();
const other = await keypair();
const keys = [{ key: good.pub }];
const bytes = release();
const sig = await signB64(good.priv, bytes);
const current = { built: "2026-10-29T10:00:00Z" };

// ── signature ──
let v = await R.verifyRelease(subtle, bytes, sig, keys, current);
ok(v.ok && v.key === good.pub, "a release signed with a trusted key is adopted");
ok(v.ok && (await R.checkFile(subtle, v.release, "index.html", files["index.html"])).ok, "…and its files match their hashes");

v = await R.verifyRelease(subtle, bytes, await signB64(other.priv, bytes), keys, current);
ok(!v.ok && v.kind === "bad", "signed with a key the app does not trust → refused (bad)");

v = await R.verifyRelease(subtle, bytes, sig, [{ key: other.pub }, { key: good.pub }], current);
ok(v.ok, "the right key anywhere in the list is enough (rotation keeps old and new)");

const tampered = release({ version: "0.0.5" });
v = await R.verifyRelease(subtle, tampered, sig, keys, current);
ok(!v.ok && v.kind === "bad", "release.json changed after signing → refused (bad)");

v = await R.verifyRelease(subtle, bytes, "", keys, current);
ok(!v.ok && v.kind === "unsigned", "no release.sig → refused (unsigned)");

v = await R.verifyRelease(subtle, bytes, "bm90IGEgc2lnbmF0dXJl", keys, current);
ok(!v.ok && v.kind === "bad", "release.sig that is not a signature → refused (bad)");

v = await R.verifyRelease(subtle, bytes, sig, [], current);
ok(!v.ok && v.kind === "nokey", "verifyRelease with no key verifies nothing (nokey) — `admit` decides what that means");

// ── the two modes: no release key (signing not set up) and a key (pinned) ──
v = await R.admit(subtle, bytes, "", [], current);
ok(v.ok && v.signing === false, "no release key: an unsigned release is taken (signing not set up)");
v = await R.admit(subtle, bytes, await signB64(other.priv, bytes), [], current);
ok(v.ok && v.signing === false, "no release key: a release signed by anyone is taken too");
v = await R.admit(subtle, bytes, "", [], { built: "2026-10-31T00:00:00Z" });
ok(v.ok, "no release key: an older release is taken (a Pages rollback works)");
v = await R.admit(subtle, enc.encode("<html>404</html>"), "", [], current);
ok(!v.ok && v.kind === "shape", "no release key: something that is not a release is still not taken");
v = await R.admit(subtle, bytes, "", keys, current);
ok(!v.ok && v.kind === "unsigned" && v.signing, "with a key: an unsigned release is refused");
v = await R.admit(subtle, bytes, sig, keys, current);
ok(v.ok && v.signing, "with a key: a signed release is taken");
v = await R.admit(subtle, bytes, sig, keys, { built: "2026-10-31T00:00:00Z" });
ok(!v.ok && v.kind === "rollback", "with a key: an older signed release is refused");
v = await R.admit(subtle, bytes, "", R.trusted([{ key: good.pub, dev: true }], "sandptel.github.io"), current);
ok(v.ok && v.signing === false, "a DEV key alone on a deployed address leaves signing off");
v = await R.admit(subtle, bytes, "", R.trusted([{ key: good.pub, dev: true }], "localhost"), current);
ok(!v.ok && v.kind === "unsigned", "a DEV key on localhost switches signing on");

// ── file hashes ──
const r = R.parseRelease(bytes).release;
const evil = enc.encode("console.log('evil')");
const fc = await R.checkFile(subtle, r, "assets/app.js", evil);
ok(!fc.ok && fc.kind === "hash", "a file whose bytes differ from its signed hash → refused (hash)");

// ── age and shape ──
v = await R.verifyRelease(subtle, bytes, sig, keys, { built: "2026-10-31T00:00:00Z" });
ok(!v.ok && v.kind === "rollback", "a signed release older than the one in use → refused (rollback)");

const sneaky = release({ files: { ...hashes, "../etc/passwd": hashes["index.html"] } });
v = await R.verifyRelease(subtle, sneaky, await signB64(good.priv, sneaky), keys, current);
ok(!v.ok && v.kind === "shape", "a path escaping the app's folder → refused (shape)");

v = await R.verifyRelease(subtle, enc.encode("<html>404</html>"), sig, keys, current);
ok(!v.ok && v.kind === "shape", "an HTML 404 page in place of release.json → refused (shape)");

// ── DEV keys ──
const devKeys = [{ key: good.pub, dev: true }];
ok(R.trusted(devKeys, "localhost").length === 1, "a DEV key is trusted on localhost");
ok(R.trusted(devKeys, "127.0.0.1").length === 1, "…and on 127.0.0.1");
ok(R.trusted(devKeys, "sandptel.github.io").length === 0, "…and never on a deployed address");
v = await R.verifyRelease(subtle, bytes, sig, R.trusted(devKeys, "sandptel.github.io"), current);
ok(!v.ok && v.kind === "nokey", "deployed, a DEV signature verifies against nothing (no key to check with)");

// ── the keys file shipped in the client parses and holds no DEV key ──
const ctx = { self: {} };
new Function("self", readFileSync(new URL("../crates/client/public/sw-keys.js", import.meta.url), "utf8"))(ctx.self);
const shipped = ctx.self.WADO_RELEASE_KEYS;
ok(Array.isArray(shipped), "public/sw-keys.js defines WADO_RELEASE_KEYS");
ok(Array.isArray(shipped) && !shipped.some((k) => k && k.dev), "public/sw-keys.js ships no DEV key");

ok((await R.keyId(subtle, good.pub)).length === 9, "a key's short name reads like \"3f9a 21c0\"");

console.log(failed ? `\n${failed} FAILED` : "\nall passed");
process.exit(failed ? 1 : 0);
