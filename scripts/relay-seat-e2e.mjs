// End-to-end check of the relay's seats, against the real binaries.
//
//   node scripts/relay-seat-e2e.mjs            (needs target/release/{wado,wado-relay})
//
// Starts a relay on a spare port and two idle daemons (no session is ever started, so no GPU is
// touched) with a throwaway config dir — the trust list written here is not yours. Clients are
// plain WebSockets speaking the join protocol. Covers, in order (WADO_PLAN.md Decision Log
// 2026-10-02):
//
//   park       a client that arrives first waits, and is paired when a daemon registers
//   gate       (Decision Log 2026-10-27) a device gets in only with a code shown on the computer:
//              the first one too; an unknown one is refused at once with how to pair; a typed
//              code pairs only once the check words are confirmed at `wado pair` (2026-10-30) —
//              a "no" there refuses it; five wrong typed codes revoke every live code
//   hold       a dropped device keeps its seat; another device is refused, the owner reclaims
//   takeover   one tap moves a seat; the displaced device is told; an unknown device is refused
//              without disturbing it
//   away       a daemon restart parks its client, which gets the same daemon back
//   leave      a client closing with the leave code frees its seat instead of holding it
//   silence    a frozen daemon is noticed and its client released
//   ratelimit  a burst of joins from one address is cut off with a retry time
//   envelope   (Decision Log 2026-10-04) the relay can neither read nor inject: plaintext and
//              tampered frames are dropped, a device claiming another's id is refused, a
//              changed computer identity is refused by the client, a QR code pairs and pins
//   control    (2026-10-30) a widget's control-only link takes no seat: it gets in beside a full
//              pool, displaces nobody, reads the computer's state, is refused anything that is
//              not a control, and is not given to a device the computer does not know
//
// Clients run the real browser envelope code (scripts/lib/e2e-device.mjs). Messages for the
// computer are sealed; `__up` is pushed when the secure link is up.
//
// Takes about 70 s, most of it waiting out the 45 s silence window.

import { spawn } from "node:child_process";
import { mkdtempSync, rmSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { execFileSync } from "node:child_process";
import { device, onFrame } from "./lib/e2e-device.mjs";

const ROOT = new URL("..", import.meta.url).pathname;
const PORT = 4977;
const RELAY = `ws://127.0.0.1:${PORT}`;
const RID = "999000111";
const CFG = mkdtempSync(join(tmpdir(), "wado-seat-e2e-"));
const procs = new Set();
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

let failures = 0;
const check = (name, ok, detail = "") => {
  if (!ok) failures++;
  console.log(`${ok ? "  ok  " : "FAIL  "}${name}${ok || !detail ? "" : "\n        " + detail}`);
};

function start(bin, args, env, log) {
  const p = spawn(join(ROOT, "target/release", bin), args, {
    env: { ...process.env, ...env },
    stdio: ["ignore", "pipe", "pipe"],
  });
  p.out = "";
  const take = (d) => { p.out += d.toString().replace(/\x1b\[[0-9;]*m/g, ""); };
  p.stdout.on("data", take);
  p.stderr.on("data", take);
  p.log = log;
  procs.add(p);
  p.on("exit", () => procs.delete(p));
  return p;
}
const relay = (extra = []) => start("wado-relay", ["--bind", `127.0.0.1:${PORT}`, ...extra], {}, "relay");
const daemon = (n) =>
  start("wado", ["daemon"], {
    WADO_RELAY_URL: RELAY,
    WADO_REMOTE_ID: RID,
    WADO_INSTANCE: String(n),
    WADO_UDP_SLICE: String(10 + n),
    XDG_CONFIG_HOME: CFG,
  }, `daemon${n}`);
async function waitFor(p, re, ms = 15000) {
  const t0 = Date.now();
  while (Date.now() - t0 < ms) {
    if (re.test(p.out)) return true;
    await sleep(100);
  }
  return false;
}
const count = (p, re) => (p.out.match(new RegExp(re, "g")) || []).length;
async function stop(p) {
  if (!p || p.exitCode !== null) return;
  p.kill("SIGTERM");
  await new Promise((r) => { p.once("exit", r); setTimeout(r, 5000); });
}

// One "browser" per client key unless a test hands in another: same key, same device key.
const devices = {};
// A fresh pairing code from `wado qr`, as the computer's screen shows it: the QR link's code and
// pin, and the same code to type.
const mint = () => {
  const out = execFileSync(join(ROOT, "target/release/wado"), ["qr", "--relay", "http://r.example", "--id", RID],
    { env: { ...process.env, XDG_CONFIG_HOME: CFG } }).toString().replace(/\x1b\[[0-9;]*m/g, "");
  const q = new URLSearchParams(out.match(/open: \S+\?(\S+)/)[1]);
  return { out, pair: q.get("pair"), hk: q.get("hk"), typed: (out.match(/type this code on it: (\S+)/) || [])[1] };
};
// `wado pair --wait`, as a person at the computer runs it: its output so far, the typed code it
// shows, and `say(line)` to answer the check-words question on its stdin.
async function pairAt() {
  const p = spawn(join(ROOT, "target/release/wado"), ["pair", "--wait", "--relay", "http://r.example", "--id", RID],
    { env: { ...process.env, XDG_CONFIG_HOME: CFG }, stdio: ["pipe", "pipe", "pipe"] });
  p.out = "";
  p.stdout.on("data", (d) => { p.out += d.toString().replace(/\x1b\[[0-9;]*m/g, ""); });
  procs.add(p);
  p.on("exit", () => procs.delete(p));
  await waitFor(p, /type this code on it: \S+/, 5000);
  p.typed = (p.out.match(/type this code on it: (\S+)/) || [])[1] || "";
  p.say = (line) => p.stdin.write(line + "\n");
  return p;
}
const trustList = () => { try { return readFileSync(join(CFG, "wado/trusted_clients"), "utf8"); } catch { return ""; } };
// A client: records every frame (sealed ones opened), answers pings.
class Client {
  constructor(name, { key = name + "-key", instance = "", takeover = false, dev = null, pair = false, control = false } = {}) {
    this.name = name;
    this.msgs = [];
    this.closed = false;
    this.dev = dev || (devices[key] ||= device(key));
    const q = new URLSearchParams({ client: key, name });
    if (instance) q.set("instance", instance);
    if (takeover) q.set("takeover", "1");
    if (pair) q.set("pair", "1");
    if (control) q.set("control", "1");
    this.ws = new WebSocket(`${RELAY}/join/${RID}?${q}`);
    this.ws.onmessage = (e) => {
      const m = JSON.parse(e.data);
      if (m.type === "ping") return this.raw({ type: "pong" });
      if (!onFrame(this.dev, this.ws, RID, m, (x) => this.msgs.push(x))) this.msgs.push(m);
    };
    this.ws.onclose = () => { this.closed = true; };
  }
  // Sealed, as the client sends everything for the computer.
  send(m) { return this.dev.W.e2eSend(m); }
  // In the clear, as the relay (or anything in front of it) could.
  raw(m) { try { this.ws.send(typeof m === "string" ? m : JSON.stringify(m)); } catch (_) {} }
  up(ms = 8000) { return this.wait("__up", ms); }
  has(type) { return this.msgs.find((m) => m.type === type); }
  wait(type, ms = 10000) { return this.waitWhere((m) => m.type === type, ms); }
  async waitWhere(pred, ms = 10000) {
    const t0 = Date.now();
    while (Date.now() - t0 < ms) {
      const m = this.msgs.find(pred);
      if (m) return m;
      await sleep(50);
    }
    return null;
  }
  async waitClosed(ms = 10000) {
    const t0 = Date.now();
    while (Date.now() - t0 < ms && !this.closed) await sleep(50);
    return this.closed;
  }
  close() { this.ws.close(); }
}

try {
  let r = relay();
  await sleep(400);

  // ── park ────────────────────────────────────────────────────────────────────
  console.log("park");
  // The phone scanned the computer's QR: nothing gets in without a code, the first device too.
  const first = mint();
  devices["Phone-key"] = device("Phone-key");
  devices["Phone-key"].store.setItem("wado.pair", first.pair);
  devices["Phone-key"].store.setItem("wado.hostpins", JSON.stringify({ [RID]: { pin: first.hk, from: "qr" } }));
  const phone = new Client("Phone", { pair: true });
  const w = await phone.wait("waiting");
  check("a client that arrives first is parked, not refused", !!w && !phone.has("join_denied"), JSON.stringify(phone.msgs));
  let d1 = daemon(1);
  const acc = await phone.wait("join_accepted", 20000);
  check("…and paired when the daemon registers", !!acc, JSON.stringify(phone.msgs));
  check("…on a stable instance id", acc && acc.instance_id === `${RID}:1`, acc && acc.instance_id);
  check("…with the relay's caps", acc && ["park", "hold", "takeover", "gate", "ping", "leave"].every((c) => acc.caps.includes(c)));
  check("…and the envelope comes up", !!(await phone.up()), JSON.stringify(phone.msgs) + "\n" + phone.dev.W.log.join("\n"));

  // ── gate ────────────────────────────────────────────────────────────────────
  console.log("gate");
  check("the first device got in with its code, and is pinned", /Phone-key\tPhone\t\S+\t1/.test(trustList()), trustList());
  check("…which is spent", !trustList().includes("Laptop") && devices["Phone-key"].store.getItem("wado.pair") === null);
  const d2 = daemon(2);
  await waitFor(d2, /registered — Remote ID/);
  const laptop = new Client("Laptop");
  const lden = await laptop.wait("join_denied", 5000);
  check("an unknown device with no code is refused at once", !!lden && !laptop.has("join_accepted"), JSON.stringify(laptop.msgs));
  check("…told how to pair", lden && /doesn't know this device yet/.test(lden.reason) && /wado pair/.test(lden.reason), lden && lden.reason);
  check("…not left waiting for an approval", !laptop.has("waiting"));
  check("…and no connected device is asked", !phone.has("approve_request"));
  check("…and nothing is trusted", !trustList().includes("Laptop-key"));
  await laptop.waitClosed(3000);
  await sleep(300);
  // The laptop types the code from the computer's screen.
  const second = await pairAt();
  check("`wado pair` shows a typed code, grouped", /^[0-9A-HJKMNP-TV-Z]{4}-[0-9A-HJKMNP-TV-Z]{4}$/.test(second.typed), second.out);
  check("…and how long it has", /expires in (10:00|9:\d\d)/.test(second.out), second.out);
  devices["Laptop-key"].store.setItem("wado.pairTyped", second.typed.replace("-", ""));
  const laptopIn = new Client("Laptop", { pair: true });
  const lacc = await laptopIn.wait("join_accepted", 5000);
  check("a typed code is let through to prove itself, on the other daemon", lacc && lacc.instance_id === `${RID}:2`, lacc && lacc.instance_id);
  const sas = await laptopIn.wait("e2e_sas", 8000);
  check("…then the computer reveals the nonce for the check words", !!sas, JSON.stringify(laptopIn.msgs));
  await waitFor(second, /Does it show these four words\?\n\s+(\S+ ){3}\S+/, 5000);
  const atComputer = (second.out.match(/Does it show these four words\?\n\s+((\S+ ){3}\S+)/) || [])[1];
  await sleep(200);
  const onDevice = (devices["Laptop-key"].W.events.filter((e) => e.type === "pair_sas").at(-1) || {}).words;
  check("…both screens show the same four words", !!atComputer && onDevice && onDevice.join(" ") === atComputer,
    `${atComputer} | ${onDevice}`);
  check("…and the device is not in, nor trusted, before anyone answers",
    !laptopIn.has("__up") && !trustList().includes("Laptop-key"), trustList());
  second.say("y");
  check("a yes at the computer lets it in", !!(await laptopIn.up()), JSON.stringify(laptopIn.msgs) + devices["Laptop-key"].W.log.join("\n"));
  check("…and the trust list pins its key", /Laptop-key\tLaptop\t\S+\t1/.test(trustList()), trustList());
  check("…and `wado pair` says so", await waitFor(second, /paired/, 3000), second.out);
  check("…the code never reached the relay", !(await waitFor(r, new RegExp(second.typed.replace("-", "")), 300)));
  laptopIn.close();
  await sleep(300);
  // Guessing. Daemon 2's seat is now *held* for the laptop, so the guesses need a free daemon:
  // replace daemon 2 with a third.
  await stop(d2);
  const d3 = daemon(3);
  await waitFor(d3, /registered — Remote ID/);
  // A "no" at the computer: the device is refused, told why, and trusts nothing.
  const fourth = await pairAt();
  devices["Desk-key"] = device("Desk-key");
  devices["Desk-key"].store.setItem("wado.pairTyped", fourth.typed.replace("-", ""));
  const desk = new Client("Desk", { pair: true });
  await waitFor(fourth, /Does it show these four words/, 8000);
  fourth.say("n");
  const dfail = await desk.wait("e2e_fail", 8000);
  check("a no at the computer refuses the device", !!dfail && /wasn't paired/.test(dfail.reason) && !desk.has("__up"),
    JSON.stringify(desk.msgs));
  check("…trusts nothing", !trustList().includes("Desk-key"), trustList());
  check("…and spends the code", await waitFor(fourth, /declined/, 3000), fourth.out);
  await desk.waitClosed(3000);
  await sleep(300);
  const third = mint();
  const guessed = [];
  for (let i = 0; i < 5; i++) {
    const dev = device(`Guess${i}-key`);
    dev.store.setItem("wado.pairTyped", "ZZZZZZZZ");
    const g = new Client(`Guess${i}`, { dev, pair: true });
    const f = await g.wait("e2e_fail", 8000);
    guessed.push(!!f && /code didn't work/.test(f.reason));
    await g.waitClosed(3000);
    await sleep(200);
  }
  check("a wrong typed code is refused, five times", guessed, [true, true, true, true, true]);
  check("…and the fifth revokes every live code", await waitFor(d3, /every live code is revoked/, 3000));
  devices["Tab-key"] = device("Tab-key");
  devices["Tab-key"].store.setItem("wado.pairTyped", third.typed.replace("-", ""));
  const late = new Client("Tab", { pair: true });
  const lf = await late.wait("e2e_fail", 8000);
  check("…so even the right code no longer works", !!lf && !late.has("__up"), JSON.stringify(late.msgs));
  check("…and nobody was trusted for guessing", !/Guess|Tab-key/.test(trustList()), trustList());
  await late.waitClosed(3000);
  await stop(d3);

  // ── hold ────────────────────────────────────────────────────────────────────
  console.log("hold");
  phone.close();
  await sleep(300);
  const tablet = new Client("Tablet");
  // Tablet is unknown: with daemon 1 held for the phone it must be refused as occupied,
  // never assigned the phone's desktop.
  const tden = await tablet.wait("join_denied", 5000);
  check("a dropped device's seat is held: another device is refused", !!tden && /in use by Phone/.test(tden.reason), tden && tden.reason);
  check("…with the takeover offer", tden && tden.takeover === true);
  check("…and the pinned wording the old clients match", tden && /already has an active connection/.test(tden.reason));
  const phone2 = new Client("Phone", { instance: `${RID}:1` });
  const pacc = await phone2.wait("join_accepted", 5000);
  check("the owner comes back to its own seat", pacc && pacc.assignment === "reclaimed", pacc && pacc.assignment);

  // ── takeover ────────────────────────────────────────────────────────────────
  console.log("takeover");
  // An unknown device cannot knock a viewer off: refused before anyone is displaced.
  const intruder = new Client("Intruder", { takeover: true });
  const iden = await intruder.wait("join_denied", 5000);
  check("an unknown device's takeover is refused at once", !!iden && /isn't paired yet/.test(iden.reason),
    JSON.stringify(intruder.msgs));
  check("…and the viewer keeps its seat, unasked", !phone2.closed && !phone2.has("taken_over") && !phone2.has("approve_request"));
  const laptop2 = new Client("Laptop", { takeover: true });
  const tacc = await laptop2.wait("join_accepted", 5000);
  check("one tap takes the seat (a trusted device, so no prompt)", tacc && tacc.assignment === "taken over", JSON.stringify(laptop2.msgs));
  const told = await phone2.wait("taken_over", 3000);
  check("…the displaced device is told who took it", told && told.by === "Laptop", JSON.stringify(phone2.msgs.slice(-2)));
  check("…and its link is closed", await phone2.waitClosed(3000));
  const twotabs = new Client("Laptop");
  const ttd = await twotabs.wait("join_denied", 4000);
  check("a second tab with the same key never steals a live seat", !!ttd);

  // ── away ────────────────────────────────────────────────────────────────────
  console.log("away");
  const regs = count(d1, "registered — Remote ID");
  await stop(d1);
  check("a daemon going away closes its client's link", await laptop2.waitClosed(5000));
  const back = new Client("Laptop", { instance: `${RID}:1` });
  const bw = await back.wait("waiting", 5000);
  check("…which parks on its held seat", bw && /reconnecting/.test(bw.reason), bw && bw.reason);
  d1 = daemon(1);
  const bacc = await back.wait("join_accepted", 20000);
  check("…and gets the same daemon back when it returns", bacc && bacc.instance_id === `${RID}:1` && bacc.assignment === "reclaimed", bacc && JSON.stringify(bacc));
  check("…with a new boot id (the client can say its apps closed)", bacc && acc && bacc.boot_id !== acc.boot_id);
  void regs;

  // ── leave ───────────────────────────────────────────────────────────────────
  console.log("leave");
  back.ws.close(4001, "leave");
  await back.waitClosed(3000);
  await sleep(300);
  let after = new Client("Phone");
  const lvacc = await after.wait("join_accepted", 5000);
  check("a device that leaves frees its seat at once: the next one walks in", !!lvacc && !after.has("join_denied"), JSON.stringify(after.msgs));

  // ── envelope ────────────────────────────────────────────────────────────────
  console.log("envelope");
  check("the phone's link is sealed", !!(await after.up()), after.dev.W.log.join("\n"));
  // What a hostile relay would inject, in the clear: dropped, never acted on.
  after.raw({ type: "session_launch", command: "touch /tmp/wado-e2e-pwned" });
  // A sealed frame with a forged counter (the relay replaying or reordering).
  after.raw({ type: "sealed", n: 999, c: "AAAA" });
  check("an injected plaintext command is dropped by the daemon",
    await waitFor(d1, /e2e: dropped a frame .*refused unsealed/, 3000));
  check("…and a forged sealed frame breaks the link instead of being read",
    await waitFor(d1, /e2e: dropped a frame .*does not open/, 3000));
  after.ws.close(4001, "leave");
  await after.waitClosed(3000);
  await sleep(300);
  // The relay names the phone, but the device behind it holds a different key.
  const impostor = new Client("Phone", { dev: device("Phone-key") });
  const fail = await impostor.wait("e2e_fail", 8000);
  check("a device claiming a trusted id with another key is refused", fail && /not the one/.test(fail.reason), JSON.stringify(impostor.msgs));
  check("…and never gets the link up", !impostor.has("__up"));
  impostor.ws.close(4001, "leave");
  await impostor.waitClosed(3000);
  await sleep(300);
  // The client refuses a computer whose identity is not the pinned one — the trusted phone,
  // with its pin swapped as if a relay were standing in for the computer.
  const real = devices["Phone-key"];
  const pinsWere = real.store.getItem("wado.hostpins");
  real.store.setItem("wado.hostpins", JSON.stringify({ [RID]: { pin: "not-this-computer", from: "qr" } }));
  const wary = new Client("Phone");
  check("a client refuses a computer whose identity changed", await wary.waitClosed(8000) && !wary.has("__up"),
    real.W.log.slice(-3).join("\n"));
  check("…and says so", real.W.log.some((l) => /identity has changed/.test(l)));
  real.store.setItem("wado.hostpins", pinsWere);
  await sleep(300);
  // A QR code: pairs a new device, pins both ends.
  const fresh = mint();
  const qr = fresh.out;
  const tab = device("Tab-key");
  tab.store.setItem("wado.pair", fresh.pair);
  tab.store.setItem("wado.hostpins", JSON.stringify({ [RID]: { pin: fresh.hk, from: "qr" } }));
  const paired = new Client("Tab", { dev: tab, pair: true });
  check("a QR-paired device gets in, pinned", !!(await paired.up()), JSON.stringify(paired.msgs) + tab.W.log.join("\n"));
  check("…the code never went to the relay", !qr.includes("pair=1") && !(await waitFor(r, new RegExp(fresh.pair), 300)));
  check("…and the trust list pins its key",
    /Tab-key\tTab\t\S+\t1/.test(readFileSync(join(CFG, "wado/trusted_clients"), "utf8")));
  check("…and the code is used up", tab.store.getItem("wado.pair") === null);
  paired.ws.close(4001, "leave");
  await paired.waitClosed(3000);
  await sleep(300);
  after = new Client("Phone");
  check("the real phone still gets in after all that", !!(await after.up()));

  // ── control ─────────────────────────────────────────────────────────────────
  console.log("control");
  // Joins come from one address here: spaced past the relay's refill (3 s) so the rate limit
  // under test elsewhere does not refuse them.
  const spaced = () => sleep(3200);
  // The phone holds the pool's only seat (daemon 1). A widget's link from the laptop must get in
  // anyway, beside it, and leave it alone.
  const online = await (await fetch(`http://127.0.0.1:${PORT}/online/${RID}`)).json();
  check("the relay says the computer serves seatless rooms", online.control === 1 && online.daemons === 1, JSON.stringify(online));
  check("…and announces it, with strict targeting", ["control", "room_strict"].every((c) => (online.caps || []).includes(c)), JSON.stringify(online));
  await spaced();
  const widget = new Client("Laptop", { control: true });
  const wacc = await widget.wait("join_accepted", 5000);
  check("a control link is let in beside a full pool", wacc && wacc.assignment === "control", JSON.stringify(widget.msgs));
  check("…and its envelope comes up", !!(await widget.up()), JSON.stringify(widget.msgs) + devices["Laptop-key"].W.log.join("\n"));
  check("…without touching the seat's viewer", !after.closed && !after.has("taken_over"));
  // A second room on the same daemon, from another trusted device: each gets its own frames.
  await spaced();
  const widget2 = new Client("Tab", { dev: tab, control: true });
  check("a second control room opens on the same daemon", !!(await widget2.up()), JSON.stringify(widget2.msgs));
  widget.send({ type: "host_get" });
  const hs = await widget.wait("host_state", 8000);
  check("it reads the computer's state", !!hs && !!hs.state, JSON.stringify(widget.msgs.slice(-3)));
  check("…in its own room only", !widget2.has("host_state"));
  widget2.send({ type: "config_get" });
  const cs = await widget2.wait("config_state", 8000);
  check("the other room gets its own answer", !!cs && !widget.has("config_state"), JSON.stringify(widget2.msgs.slice(-3)));
  widget.send({ type: "sessions_request" });
  widget.send({ type: "session_launch", command: "touch /tmp/wado-ctl-pwned" });
  const nope = await widget.waitWhere((m) => m.type === "session_error" && /controls only/.test(m.message), 5000);
  check("…and is refused anything that is not a control", !!nope && !widget.has("sessions"), JSON.stringify(widget.msgs.slice(-3)));
  await spaced();
  const busy = new Client("Desk2");
  const bden = await busy.wait("join_denied", 5000);
  check("the pool is still counted as the phone's alone", bden && /in use by Phone/.test(bden.reason) && !/Laptop|Tab/.test(bden.reason),
    bden && bden.reason);
  await busy.waitClosed(3000);
  await spaced();
  const stranger = new Client("Stranger", { control: true });
  const sfail = await stranger.wait("e2e_fail", 5000);
  check("a device the computer does not know gets no control link", !!sfail && !stranger.has("__up"), JSON.stringify(stranger.msgs));
  await stranger.waitClosed(3000);
  widget.close();
  widget2.close();
  check("a control link going leaves nothing behind", await waitFor(d1, /control link: closed/, 3000));
  // Strict targeting: a daemon that is not there is waited for, never swapped for another.
  await spaced();
  const strictQ = new URLSearchParams({ client: "Laptop-key", name: "Laptop", control: "1", strict: "1", instance: `${RID}:9` });
  const sws = new WebSocket(`${RELAY}/join/${RID}?${strictQ}`);
  const smsgs = [];
  sws.onmessage = (e) => smsgs.push(JSON.parse(e.data));
  const t1 = Date.now();
  while (Date.now() - t1 < 4000 && !smsgs.length) await sleep(50);
  check("a strict join to an absent daemon parks, not falls back",
    smsgs.length === 1 && smsgs[0].type === "waiting" && /reconnecting/.test(smsgs[0].reason), JSON.stringify(smsgs));
  sws.close();
  await sleep(300);

  // ── silence ─────────────────────────────────────────────────────────────────
  console.log("silence (≈50 s)");
  d1.kill("SIGSTOP");
  const t0 = Date.now();
  const gone = await after.waitClosed(60000);
  check("a frozen daemon is noticed and its client released", gone, `after ${Date.now() - t0} ms`);
  check("…within the 45 s window plus a check tick", Date.now() - t0 < 56000, `${Date.now() - t0} ms`);
  d1.kill("SIGCONT");
  await stop(d1);

  // ── pong ────────────────────────────────────────────────────────────────────
  // (A unit test pins `is_pong`; here only that the relay did not choke on any of the above.)

  // ── ratelimit ───────────────────────────────────────────────────────────────
  console.log("ratelimit");
  await stop(r);
  r = relay(["--join-burst", "3", "--join-refill-secs", "60"]);
  await sleep(400);
  const burst = [0, 1, 2, 3].map((i) => new Client("Burst" + i));
  await sleep(1500);
  const limited = burst.filter((c) => c.msgs.some((m) => m.type === "join_denied" && m.retry_ms > 0));
  check("the 4th join in a burst of 3 is rate limited", limited.length === 1, JSON.stringify(burst.map((c) => c.msgs)));
  check("…with a retry time", limited[0] && limited[0].msgs[0].retry_ms > 50000);
  burst.forEach((c) => c.close());
} catch (e) {
  failures++;
  console.log("FAIL  threw: " + (e && e.stack || e));
} finally {
  for (const p of procs) { try { p.kill("SIGCONT"); p.kill("SIGKILL"); } catch (_) {} }
  rmSync(CFG, { recursive: true, force: true });
}
console.log(failures ? `\n${failures} failing` : "\nall seat checks pass");
process.exit(failures ? 1 : 0);
