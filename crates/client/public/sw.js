// wado service worker — serves the pinned client, and adopts a new one only when it is signed.
//
// Installed as an app, wado opens from this worker's cache, not from the network: the exact files
// of one release, offline included. A new deploy is downloaded in the background but only taken
// into use when its release.sig verifies against a key in sw-keys.js and every file matches the
// hash release.json names for it (sw-verify.js). Then it waits for the next launch — the page in
// front of the person is never swapped under them.
//
// Trust on first use: the very first install pins whatever the server hands it, signed or not,
// and says which. See docs/release-signing.md for what this does and does not protect.
//
// State lives in the Cache API: one cache per release (`wado-rel-<id>`, the id being release.json's
// SHA-256) and one small JSON record (`wado-meta`) naming the active, pending and previous ones.
// A deploy without release.json (`dx serve`) is not pinned at all: the worker steps aside.
importScripts("sw-keys.js", "sw-verify.js");

const R = self.WadoRelease;
const SCOPE = self.registration.scope;
const META_CACHE = "wado-meta";
const META_KEY = SCOPE + "__wado_meta.json";
const REL_PREFIX = "wado-rel-";
// Fetched by the browser itself, or always from the network: never served from a release cache.
const NOT_PINNED = new Set(["sw.js", "sw-keys.js", "sw-verify.js", "release.json", "release.sig"]);
// A launch always checks; a page coming back to the front checks at most this often.
const CHECK_EVERY_MS = 10 * 60 * 1000;

const keys = () => R.trusted(self.WADO_RELEASE_KEYS, self.location.hostname);
const url = (rel) => new URL(rel, SCOPE).href;
const relOf = (u) => {
  const s = u.split(/[?#]/)[0];
  if (!s.startsWith(SCOPE)) return null;
  return s.slice(SCOPE.length) || "index.html";
};
const cacheOf = (id) => REL_PREFIX + id.slice(0, 16);
const log = (...a) => console.log("[wado sw]", ...a);

// ── the record ───────────────────────────────────────────────────────────────────────────
let mem = null;
async function readMeta() {
  if (mem) return mem;
  const r = await (await caches.open(META_CACHE)).match(META_KEY);
  mem = r ? await r.json() : {};
  return mem;
}
// Every change to the record goes through here, one at a time, read fresh and written back.
let queue = Promise.resolve();
function mutate(fn) {
  const run = queue.then(async () => {
    mem = null;
    const m = await readMeta();
    const out = await fn(m);
    await (await caches.open(META_CACHE)).put(META_KEY,
      new Response(JSON.stringify(m), { headers: { "content-type": "application/json" } }));
    mem = m;
    return out;
  });
  queue = run.catch(() => {});
  return run;
}

const entry = (r, id, v) => ({
  id, cache: cacheOf(id), version: r.version, commit: r.commit || "", built: r.built || "",
  signed: !!(v && v.ok), key: v && v.ok ? v.key : "", dev: !!(v && v.dev), at: Date.now(),
});

// ── fetching a release ───────────────────────────────────────────────────────────────────
async function fetchRelease() {
  const res = await fetch(url("release.json"), { cache: "no-cache" });
  if (!res.ok) return null;
  const bytes = new Uint8Array(await res.arrayBuffer());
  let sig = "";
  try {
    const s = await fetch(url("release.sig"), { cache: "no-cache" });
    if (s.ok) sig = await s.text();
  } catch (_) {}
  return { bytes, sig, id: await R.sha256hex(crypto.subtle, bytes) };
}

// Every file the release names, into its own cache, each checked against its hash. A file that
// could not be fetched is `kind: "fetch"` — the network, not a verdict — and is tried again later.
async function download(release, id) {
  const name = cacheOf(id);
  await caches.delete(name);
  const cache = await caches.open(name);
  for (const path of Object.keys(release.files)) {
    if (NOT_PINNED.has(path)) continue;
    let res;
    try { res = await fetch(url(path), { cache: "no-cache" }); } catch (_) { res = null; }
    if (!res || !res.ok) {
      await caches.delete(name);
      return { ok: false, kind: "fetch", why: path + " could not be downloaded" };
    }
    const body = await res.arrayBuffer();
    const c = await R.checkFile(crypto.subtle, release, path, body);
    if (!c.ok) { await caches.delete(name); return c; }
    const type = res.headers.get("content-type");
    await cache.put(url(path), new Response(body, { headers: type ? { "content-type": type } : {} }));
  }
  return { ok: true };
}

// ── first install: trust on first use ────────────────────────────────────────────────────
async function adoptFirst() {
  let got = null;
  try { got = await fetchRelease(); } catch (_) {}
  if (!got) { log("no release.json here — not pinning (a development server?)"); return false; }
  const shape = R.parseRelease(got.bytes);
  if (!shape.ok) { log("first install:", shape.why); return false; }
  const v = await R.verifySignature(crypto.subtle, got.bytes, got.sig, keys());
  const d = await download(shape.release, got.id);
  if (!d.ok) { log("first install:", d.why); return false; }
  await mutate((m) => {
    m.active = entry(shape.release, got.id, v);
    m.firstWhy = v.ok ? "" : v.why;
    m.checked = Date.now();
  });
  log("pinned", shape.release.version, v.ok ? "(signed)" : "(unsigned, trusted on first use: " + v.why + ")");
  return true;
}

// ── later releases: signed or refused ────────────────────────────────────────────────────
async function refuse(id, v) {
  const fresh = await mutate((m) => {
    const same = m.refused && m.refused.id === id && m.refused.kind === v.kind;
    m.refused = {
      id, kind: v.kind, why: v.why, version: v.release ? v.release.version : "",
      at: same ? m.refused.at : Date.now(),
    };
    return !same;
  });
  if (fresh) console.warn("[wado sw] refused an update:", v.kind, "—", v.why);
  await broadcast();
}

async function doCheck(force) {
  let meta = await readMeta();
  if (!meta.active) {
    if (await adoptFirst()) await broadcast();
    return;
  }
  if (!force && Date.now() - (meta.checked || 0) < CHECK_EVERY_MS) return;
  let got = null;
  try { got = await fetchRelease(); } catch (_) { return; } // offline: nothing to do
  meta = await mutate((m) => { m.checked = Date.now(); return m; });
  if (!got || got.id === meta.active.id || (meta.pending && got.id === meta.pending.id)) return;

  const v = await R.verifyRelease(crypto.subtle, got.bytes, got.sig, keys(), meta.active);
  if (!v.ok) return refuse(got.id, v);
  const d = await download(v.release, got.id);
  if (!d.ok) {
    if (d.kind === "fetch") { log("update", v.release.version, "not downloaded yet:", d.why); return; }
    return refuse(got.id, { ...d, release: v.release });
  }
  const dropped = await mutate((m) => {
    const old = m.pending && m.pending.id !== got.id ? m.pending.cache : "";
    m.pending = entry(v.release, got.id, v);
    m.refused = null;
    return old;
  });
  if (dropped) await caches.delete(dropped);
  log("update", v.release.version, "verified — used from the next launch");
  await broadcast();
}
let checking = null;
const check = (force) => (checking = checking || doCheck(force).catch((e) => log("check failed:", e)).finally(() => { checking = null; }));

// ── the next launch takes the pending release ────────────────────────────────────────────
// A navigation with no other window open is a launch (or a reload of the only one): the moment
// nothing is running on the old files. The previous release's cache is kept one more round, so
// a straggling tab still finds its files.
async function maybePromote() {
  if (!(await readMeta()).pending) return;
  const wins = await self.clients.matchAll({ type: "window", includeUncontrolled: true });
  if (wins.length > 1) return;
  const now = await mutate((m) => {
    if (!m.pending) return "";
    m.previous = m.active ? m.active.cache : "";
    m.active = { ...m.pending, at: Date.now() };
    m.pending = null;
    return m.active.version;
  });
  if (now) { log("now using", now); await prune(); }
}

async function prune() {
  const m = await readMeta();
  const keep = new Set([m.active && m.active.cache, m.pending && m.pending.cache, m.previous]);
  for (const n of await caches.keys()) if (n.startsWith(REL_PREFIX) && !keep.has(n)) await caches.delete(n);
}

async function fromPinned(rel) {
  const m = await readMeta();
  if (!m.active) return null;
  for (const n of [m.active.cache, m.previous].filter(Boolean)) {
    const hit = await (await caches.open(n)).match(url(rel));
    if (hit) return hit;
  }
  return null;
}

async function serve(req, rel) {
  if (req.mode === "navigate") {
    try { await maybePromote(); } catch (e) { log("promote failed:", e); }
    return (await fromPinned("index.html")) || fetch(req);
  }
  return (await fromPinned(rel)) || fetch(req);
}

// ── what the page is told ────────────────────────────────────────────────────────────────
async function status() {
  const m = await readMeta();
  const a = m.active;
  return {
    type: "wado-sw",
    pinned: !!a,
    version: a ? a.version : "",
    commit: a ? a.commit : "",
    signed: !!(a && a.signed),
    dev: !!(a && a.dev),
    updated: a ? a.at : 0,
    key: a && a.key ? await R.keyId(crypto.subtle, a.key) : "",
    keys: keys().length,
    firstWhy: m.firstWhy || "",
    pending: m.pending ? m.pending.version : "",
    pendingId: m.pending ? m.pending.id : "",
    refused: m.refused || null,
  };
}
async function broadcast() {
  const s = await status();
  for (const c of await self.clients.matchAll({ type: "window", includeUncontrolled: true })) c.postMessage(s);
}

// ── lifecycle ────────────────────────────────────────────────────────────────────────────
self.addEventListener("install", (e) => e.waitUntil((async () => {
  // The first worker pins a release and takes over at once. A newer copy of this script (the
  // browser fetches it itself) waits for the next launch like everything else.
  if (!(await readMeta()).active) {
    await adoptFirst().catch((err) => log("first install failed:", err));
    await self.skipWaiting();
  }
})()));

self.addEventListener("activate", (e) => e.waitUntil((async () => {
  await self.clients.claim();
  await prune();
  await broadcast();
})()));

self.addEventListener("message", (e) => {
  const d = e.data || {};
  if (d.type === "hello") {
    // A launch: say what is in use, then look for a newer release.
    e.waitUntil((async () => {
      if (e.source) e.source.postMessage(await status());
      await check(true);
    })());
  } else if (d.type === "check") {
    e.waitUntil(check(false));
  }
});

self.addEventListener("fetch", (e) => {
  const req = e.request;
  if (req.method !== "GET") return;
  // Nothing pinned (a development server): step aside entirely.
  if (mem && !mem.active) return;
  const rel = relOf(req.url);
  if (rel === null || NOT_PINNED.has(rel)) return;
  e.respondWith(serve(req, rel));
});
