// wado service worker — checking a release: its signature, its shape and every file's hash.
//
// Pure functions over bytes and a WebCrypto `subtle`, so the same file runs in the service worker
// (`importScripts`) and in `node scripts/sw-check.mjs`. Nothing here fetches or caches.
//
// A release is two files at the root of the deploy (written by `scripts/release-sign.mjs`):
//   release.json — {app, version, commit, built, files: {path: sha256 hex}}
//   release.sig  — base64 of the Ed25519 signature over release.json's exact bytes
// Every answer is {ok:true, ...} or {ok:false, kind, why}; `kind` is one of
//   unsigned    no release.sig was published
//   bad         a signature that no trusted key made, or one that is not a signature at all
//   nokey       this app trusts no release key, so nothing can be adopted
//   unsupported this browser cannot check an Ed25519 signature
//   shape       release.json is not a release
//   rollback    a signed release older than the one in use
//   hash        a file's bytes are not the ones release.json names
(function (root) {
  const LOCAL = new Set(["localhost", "127.0.0.1", "[::1]", "::1"]);

  function b64(text) {
    const s = atob(String(text).trim());
    const out = new Uint8Array(s.length);
    for (let i = 0; i < s.length; i++) out[i] = s.charCodeAt(i);
    return out;
  }

  const hex = (buf) => [...new Uint8Array(buf)].map((b) => b.toString(16).padStart(2, "0")).join("");

  async function sha256hex(subtle, bytes) {
    return hex(await subtle.digest("SHA-256", bytes));
  }

  // The keys this copy of the app may trust. A DEV key (`dev: true`, written into a local build
  // by `release-sign.mjs dev`) counts only on this machine's own address, so a dev key that ever
  // reached a deploy would still be worth nothing there.
  function trusted(keys, hostname) {
    return (Array.isArray(keys) ? keys : []).filter(
      (k) => k && typeof k.key === "string" && (!k.dev || LOCAL.has(String(hostname))));
  }

  // A short, readable name for a public key: the first 8 hex of its SHA-256, as "3f9a 21c0".
  async function keyId(subtle, keyB64) {
    const h = await sha256hex(subtle, b64(keyB64));
    return h.slice(0, 4) + " " + h.slice(4, 8);
  }

  async function verifySignature(subtle, bytes, sigText, keys) {
    if (!sigText || !String(sigText).trim()) return { ok: false, kind: "unsigned", why: "no signature was published" };
    let sig;
    try { sig = b64(sigText); } catch (_) { return { ok: false, kind: "bad", why: "the signature is not readable" }; }
    if (sig.length !== 64) return { ok: false, kind: "bad", why: "the signature is not an Ed25519 signature" };
    if (!keys.length) return { ok: false, kind: "nokey", why: "this app has no release key to check it with" };
    for (const k of keys) {
      let key;
      try {
        key = await subtle.importKey("raw", b64(k.key), { name: "Ed25519" }, false, ["verify"]);
      } catch (e) {
        // A malformed key in the list is skipped; a browser without Ed25519 fails every key.
        if (e && e.name === "NotSupportedError") {
          return { ok: false, kind: "unsupported", why: "this browser cannot check Ed25519 signatures" };
        }
        continue;
      }
      let good = false;
      try { good = await subtle.verify({ name: "Ed25519" }, key, sig, bytes); } catch (_) {}
      if (good) return { ok: true, key: k.key, dev: !!k.dev };
    }
    return { ok: false, kind: "bad", why: "the signature does not match any trusted release key" };
  }

  // release.json, read and checked for shape: a path is relative, has no `..`, and names a hash.
  function parseRelease(bytes) {
    let r;
    try { r = JSON.parse(new TextDecoder().decode(bytes)); } catch (_) {
      return { ok: false, kind: "shape", why: "release.json is not JSON" };
    }
    if (!r || r.app !== "wado" || typeof r.version !== "string" || !r.files || typeof r.files !== "object") {
      return { ok: false, kind: "shape", why: "release.json does not describe a wado release" };
    }
    for (const [p, h] of Object.entries(r.files)) {
      if (!p || p.startsWith("/") || p.split("/").includes("..") || !/^[0-9a-f]{64}$/.test(String(h))) {
        return { ok: false, kind: "shape", why: "release.json lists a bad entry: " + p };
      }
    }
    return { ok: true, release: r };
  }

  // A signed release must not be older than the one in use: a replayed old signature would
  // otherwise walk the app back to a version with a known hole.
  function notOlder(next, current) {
    const a = Date.parse(next && next.built), b = Date.parse(current && current.built);
    if (Number.isFinite(a) && Number.isFinite(b) && a < b) {
      return { ok: false, kind: "rollback", why: "it is older than the version in use" };
    }
    return { ok: true };
  }

  // One file's bytes against release.json's hash for it.
  async function checkFile(subtle, release, path, bytes) {
    const want = release.files[path];
    const got = await sha256hex(subtle, bytes);
    if (want !== got) return { ok: false, kind: "hash", why: path + " is not the file the release names" };
    return { ok: true };
  }

  // The whole check for a release that is not the first: shape, signature, age. Files are
  // checked one by one afterwards with `checkFile`, as they are downloaded.
  async function verifyRelease(subtle, bytes, sigText, keys, current) {
    const shape = parseRelease(bytes);
    if (!shape.ok) return shape;
    const sig = await verifySignature(subtle, bytes, sigText, keys);
    if (!sig.ok) return { ...sig, release: shape.release };
    const age = notOlder(shape.release, current);
    if (!age.ok) return { ...age, release: shape.release };
    return { ok: true, release: shape.release, key: sig.key, dev: sig.dev };
  }

  root.WadoRelease = { b64, hex, sha256hex, trusted, keyId, verifySignature, parseRelease, notOlder, checkFile, verifyRelease };
})(typeof self !== "undefined" ? self : globalThis);
