// Runnable check for pairing by on-screen code (js/pair.js, its hooks in js/relay_link.js and
// js/e2e_handshake.js) — the client half of `crates/server/src/gate/pairing.rs`.
//
//   1. The refusals that open the pairing form are the daemon's own wording (read from the Rust).
//   2. A typed code is read the way the daemon reads it: case, dashes and spaces don't matter.
//   3. Refused for not being paired, the link stops redialling and the form opens; submitting a
//      code redials once, claiming a code (`pair=1`) — and the code itself never goes on the URL.
//   4. The proof: e2e_finish carries typed_mac = HMAC(k_pair, H(T2) ‖ code), checked here against
//      an independent computation of the daemon's side; the code is never in the frame, and is
//      gone from storage after one try. Then the check words: the device's are the daemon's, a
//      session's differ from another's, a nonce that was not committed to is refused, the device
//      waits until e2e_ok — and a QR pairing has none of it.
//
// Run:  node scripts/pair-check.mjs
import { readFileSync } from "node:fs";
import vm from "node:vm";

const js = (f) => readFileSync(new URL(`../crates/client/src/js/${f}`, import.meta.url), "utf8");
const { subtle } = globalThis.crypto;
const enc = new TextEncoder();

let failures = 0;
const check = (name, got, want) => {
  const ok = JSON.stringify(got) === JSON.stringify(want);
  if (!ok) failures++;
  console.log(`${ok ? "  ok  " : "FAIL  "}${name}` +
    (ok ? "" : `\n        got  ${JSON.stringify(got)}\n        want ${JSON.stringify(want)}`));
};

function storage() {
  const m = new Map();
  return {
    m,
    getItem: (k) => (m.has(k) ? m.get(k) : null),
    setItem: (k, v) => m.set(k, String(v)),
    removeItem: (k) => m.delete(k),
  };
}

// ── 1. The daemon's wording ─────────────────────────────────────────────────
// A `pub const NAME: &str = "…";` from gate/mod.rs, with Rust's `\`-newline continuation applied.
const GATE = readFileSync(new URL("../crates/server/src/gate/mod.rs", import.meta.url), "utf8");
const rustConst = (name) => {
  const m = GATE.match(new RegExp(`pub const ${name}: &str = "((?:[^"\\\\]|\\\\[\\s\\S])*)";`));
  if (!m) throw new Error(`gate/mod.rs has no ${name}`);
  return m[1].replace(/\\\n\s*/g, "");
};
{
  const ctx = vm.createContext({ W: {}, emit: () => {}, localStorage: storage() });
  vm.runInContext(js("pair.js"), ctx);
  const re = ctx.W.PAIR_REFUSAL;
  for (const n of ["UNPAIRED", "WRONG_CODE", "STALE_CODE", "DECLINED", "NO_ANSWER"]) check(`the form opens on the daemon's ${n}`, re.test(rustConst(n)), true);
  check("…but not when every desktop is busy (a code would not help now)", re.test(rustConst("BUSY_UNPAIRED")), false);
  check("…nor for a trusted device's key mismatch",
    re.test("this device's key is not the one this computer trusts for it"), false);
  check("UNPAIRED says how to pair, as decided",
    rustConst("UNPAIRED"),
    "This computer doesn't know this device yet — on the computer run `wado pair` (or open `wado tui` → Pair) and scan the code or type it here.");
}

// ── 2. Reading a typed code ─────────────────────────────────────────────────
{
  const ctx = vm.createContext({ W: {}, emit: () => {}, localStorage: storage() });
  vm.runInContext(js("pair.js"), ctx);
  const n = ctx.W.pairNormalize;
  for (const t of ["k7qf2mxp", "K7QF-2MXP", "k7qf 2mxp", " K7QF - 2mxp "]) check(`"${t}" reads as K7QF2MXP`, n(t), "K7QF2MXP");
  check("look-alikes read as digits", n("i0lo-ab12"), "1010AB12");
  check("U is not in the alphabet", n("K7QF2MXU"), "");
  check("too short is nothing", n("K7QF2MX"), "");
  check("too long is nothing", n("K7QF2MXPP"), "");
}

// ── 3. The link, refused and paired ─────────────────────────────────────────
{
  const sockets = [];
  class FakeWS {
    static CONNECTING = 0; static OPEN = 1; static CLOSING = 2; static CLOSED = 3;
    constructor(url) { this.url = url; this.readyState = 0; this.sent = []; sockets.push(this); }
    send(s) { this.sent.push(JSON.parse(s)); }
    close() { this.readyState = 3; if (this.onclose) this.onclose({ code: 1000 }); }
    open() { this.readyState = 1; if (this.onopen) this.onopen(); }
    deliver(m) { if (this.onmessage) this.onmessage({ data: JSON.stringify(m) }); }
  }
  const store = storage();
  globalThis.localStorage = store;
  const events = [];
  const W = {
    e2eBegin: () => {}, e2eSend: () => false, e2eOnSealed: () => {}, e2eOnReply: () => {},
    e2eOnFail: () => {}, e2eClose: (ws, code, reason) => ws.close(code, reason),
  };
  const emit = (e) => events.push(e);
  new Function("W", "emit", "status", "stagebar", "WebSocket", js("relay_link.js") + "\n" + js("pair.js"))(
    W, emit, () => {}, () => {}, FakeWS,
  );
  W.relayDial("https://relay.test", "872-990-894");
  check("a device with no code does not claim one", /[?&]pair=/.test(sockets[0].url), false);
  sockets[0].open();
  sockets[0].deliver({ type: "join_denied", reason: rustConst("UNPAIRED") });
  sockets[0].close();
  check("refused as unknown: the form opens with the reason",
    events.filter((e) => e.type === "unpaired").map((e) => e.reason), [rustConst("UNPAIRED")]);
  check("…and the link stops redialling (no retry would change the answer)", W._relayRetry === null, true);
  check("a code that is not one is refused by the form", W.pairWith("K7QF2MX"), false);
  check("…and nothing is dialled for it", sockets.length, 1);
  check("a real code redials at once", W.pairWith("k7qf 2mxp"), true);
  check("…once", sockets.length, 2);
  check("…claiming a code", /[?&]pair=1(&|$)/.test(sockets[1].url), true);
  check("…without the code on the URL", /K7QF|2MXP/i.test(sockets[1].url), false);
  check("…kept, normalised, for the handshake", store.getItem("wado.pairTyped"), "K7QF2MXP");
  check("…and the form closes", events.at(-1), { type: "unpaired", reason: "" });
  sockets[1].open();
  sockets[1].deliver({ type: "join_denied", reason: "no daemon" });
  sockets[1].close();
  check("an ordinary refusal still retries", W._relayRetry !== null, true);
  clearTimeout(W._relayRetry);
  delete globalThis.localStorage;
}

// ── 4. The proof, and the check words ───────────────────────────────────────
// The client's envelope code, run unchanged, against a daemon computed here from the wire spec
// (crates/protocol/src/envelope.rs).
const WORDS = readFileSync(new URL("../crates/protocol/src/sas_words.txt", import.meta.url), "utf8").trim().split(/\s+/);
const b64 = {
  enc: (u8) => Buffer.from(u8).toString("base64url"),
  dec: (s) => new Uint8Array(Buffer.from(String(s), "base64url")),
};
const cat = (...p) => { const o = new Uint8Array(p.reduce((n, x) => n + x.length, 0)); let i = 0; for (const x of p) { o.set(x, i); i += x.length; } return o; };
const lp = (s) => { const b = enc.encode(s); return cat(new Uint8Array([b.length >> 8, b.length & 255]), b); };
const sha = async (b) => new Uint8Array(await subtle.digest("SHA-256", b));
const hkdf = async (shared, salt, info, bits) => {
  const ikm = await subtle.importKey("raw", shared, "HKDF", false, ["deriveBits"]);
  return new Uint8Array(await subtle.deriveBits({ name: "HKDF", hash: "SHA-256", salt, info: enc.encode(info) }, ikm, bits));
};
const hmac = async (key, data) => new Uint8Array(await subtle.sign("HMAC",
  await subtle.importKey("raw", key, { name: "HMAC", hash: "SHA-256" }, false, ["sign"]), data));

// One device and one daemon, up to the device's e2e_finish. `stored` is what the device holds.
async function session(stored) {
  const store = storage();
  for (const [k, v] of Object.entries(stored)) store.setItem(k, v);
  const events = [];
  const W = { clientKey: "Laptop-key", log: [], SAS_WORDS: WORDS };
  W.rlog = (l) => W.log.push(l);
  const ctx = vm.createContext({
    W, crypto, atob, btoa, TextEncoder, TextDecoder, BigInt, DataView, Uint8Array, setTimeout,
    clearTimeout, localStorage: store, status: () => {}, emit: (e) => events.push(e), console,
    WebSocket: { OPEN: 1 },
  });
  vm.runInContext(["e2e_keys.js", "e2e_seal.js", "e2e_handshake.js", "pair.js"].map(js).join("\n"), ctx);
  const sent = [];
  const ws = { readyState: 1, send: (s) => sent.push(JSON.parse(s)), close() {} };
  let up = false;
  await W.e2eBegin(ws, "room-1", "872990894", () => { up = true; });
  const hello = sent.find((m) => m.type === "e2e_hello");
  // The daemon's half.
  const eph = await subtle.generateKey({ name: "X25519" }, true, ["deriveBits"]);
  const ephD = new Uint8Array(await subtle.exportKey("raw", eph.publicKey));
  const host = await subtle.generateKey({ name: "Ed25519" }, true, ["sign", "verify"]);
  const hpk = new Uint8Array(await subtle.exportKey("raw", host.publicKey));
  const ephC = b64.dec(hello.eph);
  const t1 = cat(enc.encode("wado-e2e-v1"), lp("room-1"), lp("Laptop-key"), ephC, ephD, hpk);
  const h1 = await sha(t1);
  const sig = new Uint8Array(await subtle.sign({ name: "Ed25519" }, host.privateKey, h1));
  const nonce = crypto.getRandomValues(new Uint8Array(32));
  await W.e2eOnReply({ type: "e2e_reply", eph: b64.enc(ephD), host_pk: b64.enc(hpk), sig: b64.enc(sig), sas_commit: b64.enc(await sha(nonce)) });
  const fin = sent.find((m) => m.type === "e2e_finish");
  const peer = await subtle.importKey("raw", ephC, { name: "X25519" }, false, []);
  const shared = await subtle.deriveBits({ name: "X25519", public: peer }, eph.privateKey, 256);
  const h2 = await sha(cat(t1, b64.dec(fin.dev_pk)));
  const kSas = await hkdf(shared, h1, "wado-e2e-v1 sas", 256);
  const daemonWords = [...(await hmac(kSas, cat(h2, nonce))).slice(0, 4)].map((b) => WORDS[b]);
  // Sealed e2e_ok, as the daemon sends it once the words are confirmed.
  const okm = await hkdf(shared, h1, "wado-e2e-v1", 512);
  const d2c = await subtle.importKey("raw", okm.slice(32), "AES-GCM", false, ["encrypt"]);
  const sealedOk = async () => ({ type: "sealed", n: 0,
    c: b64.enc(new Uint8Array(await subtle.encrypt({ name: "AES-GCM", iv: new Uint8Array(12) }, d2c, enc.encode('{"type":"e2e_ok"}')))) });
  return { W, ctx, store, events, fin, shared, h1, h2, nonce, daemonWords, sealedOk, isUp: () => up };
}
const settle = () => new Promise((r) => setTimeout(r, 50));
const shown = (s) => s.events.filter((e) => e.type === "pair_sas").map((e) => e.words);

{
  const s = await session({ "wado.pairTyped": "K7QF2MXP" });
  const { fin, h2 } = s;
  check("the handshake finishes with a typed proof", !!(fin && fin.typed_mac), true);
  check("…and no QR proof (it holds none)", fin && fin.pair_mac, "");
  check("…and the code itself is nowhere in the frame", /K7QF|2MXP/i.test(JSON.stringify(fin)), false);
  const kPair = await hkdf(s.shared, s.h1, "wado-e2e-v1 pair", 256);
  const verify = async (code) => {
    const k = await subtle.importKey("raw", kPair, { name: "HMAC", hash: "SHA-256" }, false, ["verify"]);
    return subtle.verify("HMAC", k, b64.dec(fin.typed_mac), cat(h2, enc.encode(code)));
  };
  check("the daemon's check of the right code passes", await verify("K7QF2MXP"), true);
  check("…and of any other code fails", await verify("K7QF2MXQ"), false);
  // Keyed with k_pair, not with anything the relay can see: the same HMAC over public bytes fails.
  const pub = await subtle.importKey("raw", enc.encode("K7QF2MXP"), { name: "HMAC", hash: "SHA-256" }, false, ["verify"]);
  check("…and it is no oracle over the public transcript",
    await subtle.verify("HMAC", pub, b64.dec(fin.typed_mac), h2), false);
  check("the typed code is tried once: gone from storage", s.store.getItem("wado.pairTyped"), null);

  // The check words.
  check("no words before the computer reveals its nonce", shown(s), []);
  await s.W.e2eOnSas({ type: "e2e_sas", nonce: b64.enc(s.nonce) });
  await settle();
  check("the device's words are the daemon's words", shown(s), [s.daemonWords]);
  check("…four of them, from the shared list", s.daemonWords.every((w) => WORDS.includes(w)) && s.daemonWords.length === 4, true);
  check("…and the device waits: not in yet", s.isUp(), false);
  s.W.e2eOnSealed(await s.sealedOk(), () => {});
  await settle();
  check("the computer's yes lets it in", s.isUp(), true);
  check("…and the words go away", shown(s).at(-1), []);

  const other = await session({ "wado.pairTyped": "K7QF2MXP" });
  await other.W.e2eOnSas({ type: "e2e_sas", nonce: b64.enc(other.nonce) });
  await settle();
  check("another session has other words", JSON.stringify(shown(other)[0]) !== JSON.stringify(shown(s)[0]), true);
}
{
  // A nonce other than the one committed to — a relay steering the words — is refused outright.
  const s = await session({ "wado.pairTyped": "K7QF2MXP" });
  await s.W.e2eOnSas({ type: "e2e_sas", nonce: b64.enc(crypto.getRandomValues(new Uint8Array(32))) });
  await settle();
  check("a nonce that was not promised shows no words", shown(s), []);
  check("…and stops the link", s.W._e2eBlocked === true && s.W.log.some((l) => /not the ones it promised/.test(l)), true);
}
{
  // The QR path is untouched: no typed proof, and a stray e2e_sas does nothing.
  const s = await session({ "wado.pair": "QRCODEQRCODEQRCODEQRCODE" });
  check("a QR pairing sends no typed proof", !!s.fin.typed_mac, false);
  await s.W.e2eOnSas({ type: "e2e_sas", nonce: b64.enc(s.nonce) });
  await settle();
  check("…and shows no check words", shown(s), []);
  s.W.e2eOnSealed(await s.sealedOk(), () => {});
  await settle();
  check("…and is in on e2e_ok, as before", s.isUp(), true);
}
{
  // Refusals that reopen the form: a wrong code, and a declined or unanswered check.
  const s = await session({ "wado.pairTyped": "K7QF2MXP" });
  await s.W.e2eOnSas({ type: "e2e_sas", nonce: b64.enc(s.nonce) });
  await settle();
  vm.runInContext("W.e2eOnFail({ reason: " + JSON.stringify(rustConst("DECLINED")) + " })", s.ctx);
  check("declined on the computer: the words go and the form reopens",
    s.events.slice(-2).map((e) => e.type === "pair_sas" ? "words:" + e.words.length : e.type), ["words:0", "unpaired"]);
  const t = await session({ "wado.pairTyped": "K7QF2MXP" });
  vm.runInContext("W.e2eOnFail({ reason: " + JSON.stringify(rustConst("WRONG_CODE")) + " })", t.ctx);
  check("a wrong code reopens the form", t.events.map((e) => e.type), ["unpaired"]);
}

console.log(failures ? `\n${failures} failure(s)` : "\nall pairing checks passed");
process.exit(failures ? 1 : 0);
