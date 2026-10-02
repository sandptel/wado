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
//   gate       the first device is trusted; a new one waits until a connected device approves
//   hold       a dropped device keeps its seat; another device is refused, the owner reclaims
//   takeover   one tap moves a seat; the displaced device is told; an unknown device must be
//              approved by the device it would displace
//   away       a daemon restart parks its client, which gets the same daemon back
//   silence    a frozen daemon is noticed and its client released
//   ratelimit  a burst of joins from one address is cut off with a retry time
//
// Takes about 70 s, most of it waiting out the 45 s silence window.

import { spawn } from "node:child_process";
import { mkdtempSync, rmSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

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
  start("wado", [], {
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

// A client: records every frame, answers pings unless told not to.
class Client {
  constructor(name, { key = name + "-key", instance = "", takeover = false } = {}) {
    this.name = name;
    this.msgs = [];
    this.closed = false;
    const q = new URLSearchParams({ client: key, name });
    if (instance) q.set("instance", instance);
    if (takeover) q.set("takeover", "1");
    this.ws = new WebSocket(`${RELAY}/join/${RID}?${q}`);
    this.ws.onmessage = (e) => {
      const m = JSON.parse(e.data);
      if (m.type === "ping") return this.send({ type: "pong" });
      this.msgs.push(m);
    };
    this.ws.onclose = () => { this.closed = true; };
  }
  send(m) { try { this.ws.send(JSON.stringify(m)); } catch (_) {} }
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
  const phone = new Client("Phone");
  const w = await phone.wait("waiting");
  check("a client that arrives first is parked, not refused", !!w && !phone.has("join_denied"), JSON.stringify(phone.msgs));
  let d1 = daemon(1);
  const acc = await phone.wait("join_accepted", 20000);
  check("…and paired when the daemon registers", !!acc, JSON.stringify(phone.msgs));
  check("…on a stable instance id", acc && acc.instance_id === `${RID}:1`, acc && acc.instance_id);
  check("…with the relay's caps", acc && ["park", "hold", "takeover", "gate", "ping"].every((c) => acc.caps.includes(c)));

  // ── gate ────────────────────────────────────────────────────────────────────
  console.log("gate");
  check("the first device was trusted (empty trust list)",
    readFileSync(join(CFG, "wado/trusted_clients"), "utf8").includes("Phone-key"));
  const d2 = daemon(2);
  await waitFor(d2, /registered — Remote ID/);
  const laptop = new Client("Laptop");
  const ask = await phone.wait("approve_request", 8000);
  check("a new device is shown to the connected one for approval", ask && ask.name === "Laptop", JSON.stringify(phone.msgs.slice(-3)));
  check("…and is not let in meanwhile", !laptop.has("join_accepted"));
  const lw = await laptop.wait("waiting", 4000);
  check("…and is told it is waiting for approval", lw && /approv/.test(lw.reason), lw && lw.reason);
  phone.send({ type: "approve_answer", id: ask.id, verdict: "always" });
  const lacc = await laptop.wait("join_accepted", 5000);
  check("approving lets it in, on the other daemon", lacc && lacc.instance_id === `${RID}:2`, lacc && lacc.instance_id);
  check("…and 'always' remembers it",
    readFileSync(join(CFG, "wado/trusted_clients"), "utf8").includes("Laptop-key"));
  check("…and the prompt is withdrawn", !!(await phone.wait("approve_cleared", 4000)));
  laptop.close();
  await sleep(300);
  // A stranger, denied. Daemon 2's seat is now *held* for the laptop, so the stranger needs a
  // free daemon to be assigned at all: replace daemon 2 with a third.
  await stop(d2);
  const d3 = daemon(3);
  await waitFor(d3, /registered — Remote ID/);
  const stranger = new Client("Stranger");
  const sask = await phone.waitWhere((m) => m.type === "approve_request" && m.name === "Stranger", 8000);
  check("a stranger is put to the connected device too", !!sask);
  if (sask) phone.send({ type: "approve_answer", id: sask.id, verdict: "deny" });
  const sden = await stranger.wait("join_denied", 5000);
  check("…and denying refuses it", !!sden && !stranger.has("join_accepted"), sden && sden.reason);
  check("…and does not trust it",
    !readFileSync(join(CFG, "wado/trusted_clients"), "utf8").includes("Stranger-key"));
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
  // An untrusted device cannot knock a viewer off: the takeover is put to that viewer first.
  const intruder = new Client("Intruder", { takeover: true });
  const iask = await phone2.waitWhere((m) => m.type === "approve_request" && m.name === "Intruder", 8000);
  check("an unknown device's takeover is put to the device it would displace", !!iask);
  check("…which keeps its seat meanwhile", !phone2.has("taken_over") && !phone2.closed);
  if (iask) phone2.send({ type: "approve_answer", id: iask.id, verdict: "deny" });
  const iden = await intruder.wait("join_denied", 5000);
  check("…and a denial leaves it where it is", !!iden && !phone2.closed && !phone2.has("taken_over"),
    JSON.stringify(intruder.msgs));
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

  // ── silence ─────────────────────────────────────────────────────────────────
  console.log("silence (≈50 s)");
  d1.kill("SIGSTOP");
  const t0 = Date.now();
  const gone = await back.waitClosed(60000);
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
