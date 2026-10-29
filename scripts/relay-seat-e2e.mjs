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
//              code pairs; five wrong typed codes revoke every live code
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
const trustList = () => { try { return readFileSync(join(CFG, "wado/trusted_clients"), "utf8"); } catch { return ""; } };
// A client: records every frame (sealed ones opened), answers pings.
class Client {
  constructor(name, { key = name + "-key", instance = "", takeover = false, dev = null, pair = false } = {}) {
    this.name = name;
    this.msgs = [];
    this.closed = false;
    this.dev = dev || (devices[key] ||= device(key));
    const q = new URLSearchParams({ client: key, name });
    if (instance) q.set("instance", instance);
    if (takeover) q.set("takeover", "1");
    if (pair) q.set("pair", "1");
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
  const second = mint();
  check("`wado qr` shows a typed code, grouped", /^[0-9A-HJKMNP-TV-Z]{4}-[0-9A-HJKMNP-TV-Z]{4}$/.test(second.typed || ""), second.out);
  check("…and how long it has", /expires in (10:00|9:\d\d)/.test(second.out), second.out);
  devices["Laptop-key"].store.setItem("wado.pairTyped", second.typed.replace("-", ""));
  const laptopIn = new Client("Laptop", { pair: true });
  const lacc = await laptopIn.wait("join_accepted", 5000);
  check("a typed code lets it in, on the other daemon", lacc && lacc.instance_id === `${RID}:2`, lacc && lacc.instance_id);
  check("…the secure link comes up", !!(await laptopIn.up()), JSON.stringify(laptopIn.msgs) + devices["Laptop-key"].W.log.join("\n"));
  check("…and the trust list pins its key", /Laptop-key\tLaptop\t\S+\t1/.test(trustList()), trustList());
  check("…the code never reached the relay", !(await waitFor(r, new RegExp(second.typed.replace("-", "")), 300)));
  laptopIn.close();
  await sleep(300);
  // Guessing. Daemon 2's seat is now *held* for the laptop, so the guesses need a free daemon:
  // replace daemon 2 with a third.
  await stop(d2);
  const d3 = daemon(3);
  await waitFor(d3, /registered — Remote ID/);
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
