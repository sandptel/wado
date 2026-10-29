#!/usr/bin/env node
// Release signing for the installed wado app (docs/release-signing.md). No dependencies: Node's
// own Ed25519 and SHA-256.
//
//   node scripts/release-sign.mjs keygen <private-key.pem>
//       Make a release key. The private half goes to the path you give (mode 0600, never
//       overwritten) — keep it offline. Prints the public half to paste into public/sw-keys.js.
//   node scripts/release-sign.mjs manifest <dir> [--version V] [--commit SHA]
//       Write <dir>/release.json: the version and the SHA-256 of every file in the tree. No key;
//       this is what CI runs after `dx build`.
//   node scripts/release-sign.mjs check <dir>
//       Recompute every hash and compare with <dir>/release.json. Prints what the release is.
//   node scripts/release-sign.mjs sign <dir> <private-key.pem>
//       `check`, then sign release.json's exact bytes: writes <dir>/release.sig and prints it.
//   node scripts/release-sign.mjs verify <dir>
//       `check`, then verify <dir>/release.sig against the keys in <dir>/sw-keys.js, the way the
//       app's service worker will. CI runs this before deploying a signature.
//   node scripts/release-sign.mjs dev <dir> [--unsigned] [--version V]
//       LOCAL TESTING ONLY. Adds a throwaway DEV key (target/release-dev-key/, never committed,
//       honoured only on localhost) to <dir>/sw-keys.js, writes release.json and — unless
//       --unsigned — signs it with that key.
import { createHash, createPrivateKey, createPublicKey, generateKeyPairSync, sign, verify } from "node:crypto";
import { existsSync, mkdirSync, readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { dirname, join, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import vm from "node:vm";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const DEV_KEY = join(ROOT, "target", "release-dev-key", "dev-key.pem");
const SKIP = new Set(["release.json", "release.sig"]);

const die = (msg) => { console.error("release-sign: " + msg); process.exit(1); };
const sha256 = (buf) => createHash("sha256").update(buf).digest("hex");
const flag = (args, name) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : undefined; };

// Every file under `dir`, as release paths (forward slashes), sorted.
function walk(dir, base = dir, out = []) {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) walk(p, base, out);
    else out.push(relative(base, p).split(sep).join("/"));
  }
  return out.sort();
}

// The raw 32-byte public key of a key object, base64 — the form sw-keys.js holds.
const rawPublic = (key) => createPublicKey(key).export({ format: "der", type: "spki" }).subarray(-32).toString("base64");
const keyId = (b64) => { const h = sha256(Buffer.from(b64, "base64")); return h.slice(0, 4) + " " + h.slice(4, 8); };
const publicFromRaw = (b64) => createPublicKey({
  key: Buffer.concat([Buffer.from("302a300506032b6570032100", "hex"), Buffer.from(b64, "base64")]),
  format: "der", type: "spki",
});

function clientVersion() {
  const toml = readFileSync(join(ROOT, "crates/client/Cargo.toml"), "utf8");
  return (toml.match(/^version\s*=\s*"([^"]+)"/m) || [])[1] || "0.0.0";
}

function manifest(dir, { version, commit } = {}) {
  const files = {};
  for (const p of walk(dir)) if (!SKIP.has(p)) files[p] = sha256(readFileSync(join(dir, p)));
  const rel = {
    app: "wado",
    version: version || clientVersion(),
    commit: commit || "",
    built: new Date().toISOString(),
    files,
  };
  writeFileSync(join(dir, "release.json"), JSON.stringify(rel, null, 1) + "\n");
  return rel;
}

// release.json against the tree: every listed file present with its hash, nothing unlisted.
function check(dir) {
  const p = join(dir, "release.json");
  if (!existsSync(p)) die(p + " is missing — run `manifest` first");
  const bytes = readFileSync(p);
  const rel = JSON.parse(bytes.toString("utf8"));
  const on = new Set(walk(dir).filter((f) => !SKIP.has(f)));
  const bad = [];
  for (const [f, h] of Object.entries(rel.files || {})) {
    if (!on.has(f)) bad.push("missing: " + f);
    else if (sha256(readFileSync(join(dir, f))) !== h) bad.push("changed: " + f);
    on.delete(f);
  }
  for (const f of on) bad.push("not in release.json: " + f);
  if (bad.length) die("the tree does not match release.json:\n  " + bad.join("\n  "));
  console.log(`release ${rel.version}  commit ${rel.commit || "(none)"}  built ${rel.built}  ` +
    `${Object.keys(rel.files).length} files  sha256 ${sha256(bytes).slice(0, 16)}…  ✓ hashes match`);
  return { bytes, rel };
}

function signDir(dir, keyPath) {
  const { bytes } = check(dir);
  const key = createPrivateKey(readFileSync(keyPath));
  if (key.asymmetricKeyType !== "ed25519") die(keyPath + " is not an Ed25519 key");
  const sig = sign(null, bytes, key).toString("base64");
  writeFileSync(join(dir, "release.sig"), sig + "\n");
  console.log(`signed with key ${keyId(rawPublic(key))}\nrelease.sig: ${sig}`);
  return sig;
}

// The keys a tree's own sw-keys.js trusts, read the way the worker reads them.
function treeKeys(dir, hostname) {
  const ctx = { self: {} };
  vm.runInNewContext(readFileSync(join(dir, "sw-keys.js"), "utf8"), ctx);
  return (ctx.self.WADO_RELEASE_KEYS || []).filter((k) => k && k.key && (!k.dev || hostname === "localhost"));
}

function verifyDir(dir, hostname = "deployed") {
  const { bytes } = check(dir);
  const sigPath = join(dir, "release.sig");
  if (!existsSync(sigPath)) die("no release.sig in " + dir);
  const sig = Buffer.from(readFileSync(sigPath, "utf8").trim(), "base64");
  const keys = treeKeys(dir, hostname);
  if (!keys.length) die("sw-keys.js in this tree trusts no key — add your public key first");
  const good = keys.find((k) => verify(null, bytes, publicFromRaw(k.key), sig));
  if (!good) die("release.sig does not verify against any key in sw-keys.js (" + keys.map((k) => keyId(k.key)).join(", ") + ")");
  console.log(`release.sig ✓ verifies with key ${keyId(good.key)}`);
}

function keygen(out) {
  if (!out) die("keygen needs a path for the private key, e.g. ~/wado-release-key.pem");
  if (existsSync(out)) die(out + " exists — refusing to overwrite a key");
  const { privateKey } = generateKeyPairSync("ed25519");
  mkdirSync(dirname(resolve(out)), { recursive: true });
  writeFileSync(out, privateKey.export({ format: "pem", type: "pkcs8" }), { mode: 0o600 });
  const pub = rawPublic(privateKey);
  console.log(`private key written to ${out} (mode 0600). Keep it offline; never commit it.\n` +
    `public key ${keyId(pub)} — add this line to crates/client/public/sw-keys.js:\n\n` +
    `  { key: "${pub}", note: "release key, made ${new Date().toISOString().slice(0, 10)}" },\n`);
  return pub;
}

// LOCAL TESTING ONLY: a throwaway key under target/, put into this tree's sw-keys.js as `dev`.
function dev(dir, args) {
  if (!existsSync(DEV_KEY)) {
    mkdirSync(dirname(DEV_KEY), { recursive: true });
    const { privateKey } = generateKeyPairSync("ed25519");
    writeFileSync(DEV_KEY, privateKey.export({ format: "pem", type: "pkcs8" }), { mode: 0o600 });
    console.log("made a DEV key at " + DEV_KEY + " (local testing only)");
  }
  const pub = rawPublic(createPrivateKey(readFileSync(DEV_KEY)));
  const kp = join(dir, "sw-keys.js");
  let src = readFileSync(kp, "utf8");
  if (!src.includes(pub)) {
    // `dx build` minifies public/*.js, so the list may read `self.WADO_RELEASE_KEYS=[`.
    src = src.replace(/self\.WADO_RELEASE_KEYS\s*=\s*\[/,
      `self.WADO_RELEASE_KEYS = [\n  { key: "${pub}", dev: true, note: "DEV key — local testing only, honoured on localhost only" },`);
    if (!src.includes(pub)) die("could not find WADO_RELEASE_KEYS in " + kp);
    writeFileSync(kp, src);
  }
  manifest(dir, { version: flag(args, "--version"), commit: "dev" });
  if (args.includes("--unsigned")) { check(dir); console.log("left unsigned (--unsigned)"); return; }
  signDir(dir, DEV_KEY);
}

const [cmd, ...args] = process.argv.slice(2);
const dir = args[0] && resolve(args[0]);
switch (cmd) {
  case "keygen": keygen(args[0]); break;
  case "manifest": {
    const r = manifest(dir, { version: flag(args, "--version"), commit: flag(args, "--commit") });
    console.log(`release.json: ${r.version}, ${Object.keys(r.files).length} files`);
    break;
  }
  case "check": check(dir); break;
  case "sign": signDir(dir, args[1] || die("sign needs <dir> <private-key.pem>")); break;
  case "verify": verifyDir(dir); break;
  case "dev": dev(dir, args); break;
  default:
    die("usage: release-sign.mjs keygen|manifest|check|sign|verify|dev … (see the head of this file)");
}
