// End-to-end check of several devices in one session (Decision Log 2026-10-30), against the real
// binaries: a relay, one daemon, two paired devices and a legacy one.
//
//   cargo build --release -p wado -p wado-relay
//   node scripts/multi-e2e.mjs        (needs a GPU and Chrome; run with the sandbox off)
//
// Devices are the real browser envelope (scripts/lib/e2e-device.mjs); each device's media end
// is a headless Chrome page doing plain WebRTC (one RTCPeerConnection, the input and presence
// channels), driven over CDP — so frames are decoded by a real browser and input really crosses
// a data channel. Covers:
//
//   join        B joins A's session through a seatless joined room (`control=1&strict=1`,
//               `room_hello{view}`) after a speed test; A is told, both decode the one encode
//   co-control  A's and B's input both reach the seat (two origins in the arbiter's log), and
//               each sees the other's presence dot, never its own
//   take over   B takes over at its size: A is disconnected with a plain `session_left` and does
//               not retry; the session's file names B as lead at B's size
//   rejoin      A rejoins — a join: nobody is displaced
//   end         A, not the lead, ends it for everyone: B is told who
//   refusal     a speed test below the floor refuses the join it was for; a legacy device with no
//               `join` grant is refused with how to allow it
//   sibling     "Start new" on a busy daemon starts a sibling daemon and points there; the
//               sibling exits after its session ends
//
// Kills only processes it started, by pid.

import { spawn, execFileSync } from "node:child_process";
import { appendFileSync, mkdtempSync, mkdirSync, readFileSync, readdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { device, onFrame } from "./lib/e2e-device.mjs";

const ROOT = new URL("..", import.meta.url).pathname;
const BIN = join(ROOT, process.env.WADO_BIN || "target/release");
const PORT = 4983;
const RELAY = `ws://127.0.0.1:${PORT}`;
const RID = "999000777";
const T = mkdtempSync(join(tmpdir(), "wado-multi-e2e-"));
const CFG = join(T, "cfg");
const RUN = join(T, "run");
mkdirSync(join(CFG, "wado"), { recursive: true });
mkdirSync(RUN, { recursive: true, mode: 0o700 });
// A device on file from before pairing codes: trusted, no `join` grant.
// (Appended after the others pair: the first line on file is the owner, who may do anything.)
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const procs = new Set();
let failures = 0;
const check = (name, ok, detail = "") => {
  if (!ok) failures++;
  console.log(`${ok ? "  ok  " : "FAIL  "}${name}${ok || !detail ? "" : "\n        " + String(detail).slice(0, 900)}`);
};
const ENV = { XDG_CONFIG_HOME: CFG, XDG_RUNTIME_DIR: RUN };
for (const sig of ["SIGTERM", "SIGINT", "SIGHUP"]) process.on(sig, () => { cleanup(); process.exit(1); });

function start(bin, args, env = {}) {
  const p = spawn(bin, args, { env: { ...process.env, ...ENV, ...env }, stdio: ["ignore", "pipe", "pipe"] });
  p.out = "";
  const take = (d) => { p.out += d.toString().replace(/\x1b\[[0-9;]*m/g, ""); };
  p.stdout.on("data", take);
  p.stderr.on("data", take);
  procs.add(p);
  p.on("exit", () => procs.delete(p));
  return p;
}
async function waitFor(p, re, ms = 15000) {
  const t0 = Date.now();
  while (Date.now() - t0 < ms) { if (re.test(p.out)) return true; await sleep(100); }
  return false;
}
// Siblings this run's daemon started: found by the runtime dir only this run uses, killed by pid.
function siblings() {
  const out = [];
  for (const pid of readdirSync("/proc").filter((d) => /^\d+$/.test(d))) {
    try {
      const env = readFileSync(`/proc/${pid}/environ`, "utf8");
      const argv = readFileSync(`/proc/${pid}/cmdline`, "utf8").split("\0");
      if (argv[0] === join(BIN, "wado") && argv[1] === "daemon" && env.includes(`XDG_RUNTIME_DIR=${RUN}\0`) && env.includes("WADO_SIBLING=1")) out.push(+pid);
    } catch {}
  }
  return out;
}
function cleanup() {
  for (const p of procs) try { p.kill("SIGTERM"); } catch {}
  for (const pid of siblings()) try { process.kill(pid, "SIGTERM"); } catch {}
}

const mint = () => {
  const out = execFileSync(join(BIN, "wado"), ["qr", "--relay", "http://r.example", "--id", RID], { env: { ...process.env, ...ENV } })
    .toString().replace(/\x1b\[[0-9;]*m/g, "");
  const q = new URLSearchParams(out.match(/open: \S+\?(\S+)/)[1]);
  return { pair: q.get("pair"), hk: q.get("hk") };
};

const devices = {};
class Client {
  constructor(name, { key = name + "-key", instance = "", room = false, pair = null } = {}) {
    this.name = name;
    this.msgs = [];
    this.closed = false;
    this.dev = devices[key] ||= device(key);
    if (pair) {
      this.dev.store.setItem("wado.pair", pair.pair);
      this.dev.store.setItem("wado.hostpins", JSON.stringify({ [RID]: { pin: pair.hk, from: "qr" } }));
    }
    const q = new URLSearchParams({ client: key, name });
    if (instance) q.set("instance", instance);
    if (room) { q.set("control", "1"); q.set("strict", "1"); }
    if (pair) q.set("pair", "1");
    this.ws = new WebSocket(`${RELAY}/join/${RID}?${q}`);
    this.room = room;
    this.ws.onmessage = (e) => {
      const m = JSON.parse(e.data);
      if (m.type === "ping") return this.ws.send(JSON.stringify({ type: "pong" }));
      const push = (x) => {
        // A joined room says what it is for first, as relay_link.js does.
        if (x.type === "__up" && this.room) this.dev.W.e2eSend({ type: "room_hello", kind: "view" });
        this.msgs.push(x);
      };
      if (!onFrame(this.dev, this.ws, RID, m, push)) this.msgs.push(m);
    };
    this.ws.onclose = () => { this.closed = true; };
  }
  send(m) { return this.dev.W.e2eSend(m); }
  async waitWhere(pred, ms = 10000) {
    const t0 = Date.now();
    while (Date.now() - t0 < ms) { const m = this.msgs.find(pred); if (m) return m; await sleep(50); }
    return null;
  }
  wait(type, ms) { return this.waitWhere((m) => m.type === type, ms); }
  up(ms = 10000) { return this.wait("__up", ms); }
  // Clears what was seen, so the next wait is about what happens next.
  clear() { this.msgs.length = 0; }
  leave() { try { this.ws.close(4001, "leave"); } catch {} }
}

// A browser's media end: plain WebRTC in a headless Chrome page.
class Media {
  constructor(name, port) { Object.assign(this, { name, port, id: 0, pend: new Map() }); }
  async open() {
    start("google-chrome-stable", ["--headless=new", "--remote-debugging-port=" + this.port,
      "--user-data-dir=" + join(T, "chrome-" + this.name), "--autoplay-policy=no-user-gesture-required",
      "--no-first-run", "about:blank"]);
    let list;
    for (let i = 0; i < 60 && !list; i++) { await sleep(200); try { list = await (await fetch(`http://127.0.0.1:${this.port}/json/list`)).json(); } catch {} }
    this.ws = new WebSocket(list.find((t) => t.type === "page").webSocketDebuggerUrl);
    await new Promise((r) => (this.ws.onopen = r));
    this.ws.onmessage = (e) => { const m = JSON.parse(e.data); if (m.id && this.pend.has(m.id)) { this.pend.get(m.id)(m); this.pend.delete(m.id); } };
  }
  cdp(method, params = {}) { return new Promise((r) => { const i = ++this.id; this.pend.set(i, r); this.ws.send(JSON.stringify({ id: i, method, params })); }); }
  async ev(expr) { return (await this.cdp("Runtime.evaluate", { expression: expr, returnByValue: true, awaitPromise: true })).result?.result?.value; }
  offer() {
    return this.ev(`(async () => {
      if (window.pc) window.pc.close();
      const pc = window.pc = new RTCPeerConnection({ iceServers: [] });
      pc.addTransceiver("video", { direction: "recvonly" });
      window.inp = pc.createDataChannel("wado-input", { ordered: true });
      window.dots = [];
      const pres = pc.createDataChannel("wado-presence", { ordered: false, maxRetransmits: 0 });
      pres.onmessage = (e) => { try { window.dots.push(...JSON.parse(e.data).p); } catch (_) {} };
      pc.ontrack = (e) => { const v = document.createElement("video"); v.muted = true; v.autoplay = true; v.srcObject = e.streams[0]; document.body.append(v); };
      await pc.setLocalDescription(await pc.createOffer());
      await new Promise((r) => { if (pc.iceGatheringState === "complete") r(); pc.onicegatheringstatechange = () => pc.iceGatheringState === "complete" && r(); setTimeout(r, 3000); });
      return JSON.stringify(pc.localDescription);
    })()`);
  }
  answer(sdp) { return this.ev(`window.pc.setRemoteDescription(${sdp}).then(() => true)`); }
  async decoded() {
    return this.ev(`(async () => { let n = 0; (await window.pc.getStats()).forEach((r) => { if (r.type === "inbound-rtp" && r.kind === "video") n = r.framesDecoded || 0; }); return n; })()`);
  }
  // Pointer moves across the picture, on the input channel, as the client sends them.
  async point(x, y) {
    return this.ev(`(async () => { for (let i = 0; i < 6; i++) { if (window.inp.readyState === "open") window.inp.send(JSON.stringify({ t: "pointer_motion", x: ${x} + i * 0.01, y: ${y} })); await new Promise((r) => setTimeout(r, 80)); } return window.inp.readyState; })()`);
  }
  dots() { return this.ev(`window.dots.map((d) => d.name + ":" + d.kind)`); }
}

// Negotiate a device's media over its link; true once frames decode.
async function stream(c, m) {
  const sdp = await m.offer();
  c.send({ type: "sdp_offer", sdp });
  const a = await c.wait("sdp_answer", 15000);
  if (!a) return false;
  await m.answer(a.sdp);
  for (let i = 0; i < 60; i++) { if ((await m.decoded()) > 3) return true; await sleep(250); }
  return false;
}

const config = (w, h) => ({ width: w, height: h, fps: 30, quality: { custom: { bitrate_kbps: 3000 } } });
let d1;
try {
  start(join(BIN, "wado-relay"), ["--bind", "127.0.0.1:" + PORT]);
  await sleep(400);
  d1 = start(join(BIN, "wado"), ["daemon"], { WADO_RELAY_URL: RELAY, WADO_REMOTE_ID: RID, WADO_INSTANCE: "1", WADO_UDP_SLICE: "12",
    RUST_LOG: "info,wado_compositor::input::arbiter=debug" });
  check("the daemon registers", await waitFor(d1, /registered — Remote ID/), d1.out.slice(-600));

  // ── pairing: both devices by code, so both may join and take over ──
  console.log("pairing");
  const pb = mint();
  let b = new Client("Laptop", { pair: pb });
  check("B pairs with a code", !!(await b.up()), JSON.stringify(b.msgs));
  b.leave();
  await sleep(500);
  const trusted = readFileSync(join(CFG, "wado/trusted_clients"), "utf8");
  check("…and a code-paired device may join and take over", /Laptop-key\tLaptop\t\S+\t1\t[^\t]*\t\S*join,takeover/.test(trusted), trusted);
  const pa = mint();
  const a = new Client("Phone", { pair: pa });
  check("A pairs and holds the seat", !!(await a.up()), JSON.stringify(a.msgs));
  appendFileSync(join(CFG, "wado/trusted_clients"), "Lab-key\tLab\n");

  // ── A starts a session ──
  console.log("start");
  a.send({ type: "session_start", config: config(640, 360) });
  const st = await a.waitWhere((m) => m.type === "session_started" || m.type === "session_error", 30000);
  check("A starts a session", st && st.type === "session_started", JSON.stringify(st) + d1.out.slice(-500));
  const ma = new Media("a", 9361);
  const mb = new Media("b", 9362);
  await ma.open();
  await mb.open();
  check("A streams it", await stream(a, ma));

  // ── B joins through a joined room ──
  console.log("join");
  b = new Client("Laptop", { instance: `${RID}:1`, room: true });
  check("B's joined room comes up beside A's seat", !!(await b.up()), JSON.stringify(b.msgs));
  b.send({ type: "link_test", bytes: 262144 });
  const t0 = Date.now();
  let first = 0;
  const done = await b.waitWhere((m) => { if (m.type === "link_test_chunk" && !first) first = Date.now(); return m.type === "link_test_done"; }, 10000);
  const chunks = b.msgs.filter((m) => m.type === "link_test_chunk").length;
  check("the speed test pulls its burst", !!done && chunks === done.chunks && chunks >= 16, `${chunks} chunks, done=${JSON.stringify(done)}`);
  const down = Math.round((262144 * 8) / Math.max(1, Date.now() - first));
  b.send({ type: "link_report", down_kbps: down, rtt_ms: first - t0, intent: "join" });
  const v = await b.wait("link_verdict");
  check("…and the computer judges it", v && ["ok", "warn"].includes(v.verdict.level) && v.verdict.max_participants >= 2,
    JSON.stringify(v && v.verdict));
  a.clear();
  b.send({ type: "session_join" });
  check("B joins", !!(await b.wait("session_started", 10000)), JSON.stringify(b.msgs.slice(-5)));
  const joined = await a.waitWhere((m) => m.type === "participants" && m.event === "joined", 5000);
  check("A is told who joined", joined && joined.who === "Laptop" && joined.list.length === 2, JSON.stringify(joined));
  check("…with A as the lead", joined && joined.list.find((p) => p.lead)?.name === "Phone", JSON.stringify(joined));
  check("B streams the same session", await stream(b, mb));
  const before = await ma.decoded();
  await sleep(1500);
  check("…and A still does: one encode, two viewers", (await ma.decoded()) > before);

  // ── co-control ──
  console.log("co-control");
  await ma.point(0.2, 0.3);
  await mb.point(0.7, 0.6);
  await sleep(600);
  const firsts = (d1.out.match(/arbiter: first input from a device reached the seat[^\n]*origin=(\d+)|origin=(\d+)[^\n]*arbiter: first input/g) || []).length;
  check("both devices' input reaches the seat", firsts >= 2, d1.out.split("\n").filter((l) => /arbiter/.test(l)).join("\n"));
  check("no input was dropped by a lease", !/arbiter: input dropped/.test(d1.out));
  const da = await ma.dots(), db = await mb.dots();
  check("A sees B's pointer, not its own", da.some((d) => d.startsWith("Laptop:")) && !da.some((d) => d.startsWith("Phone:")), JSON.stringify(da));
  check("B sees A's pointer, not its own", db.some((d) => d.startsWith("Phone:")) && !db.some((d) => d.startsWith("Laptop:")), JSON.stringify(db));

  // ── take over ──
  console.log("take over");
  a.clear(); b.clear();
  b.send({ type: "session_takeover", config: config(800, 600) });
  check("B takes over", !!(await b.wait("session_started", 10000)), JSON.stringify(b.msgs.slice(-4)));
  const left = await a.wait("session_left", 5000);
  check("A is disconnected, in plain words", left && left.reason === "taken_over" && left.by === "Laptop", JSON.stringify(left));
  await sleep(5000);
  check("…and nothing is retried on A's behalf", !a.msgs.some((m) => m.type === "session_started"));
  b.clear();
  b.send({ type: "sessions_request" });
  const ss = await b.wait("sessions");
  const s1 = ss && ss.sessions.find((s) => s.instance === `${RID}:1`);
  check("the session is B's now, at B's size", s1 && s1.lead === "Laptop" && s1.width === 800 && s1.height === 600 && s1.participants.length === 1,
    JSON.stringify(s1));
  check("…rebuilt in place (a fresh output, the apps kept)", /reconfigur|output/i.test(d1.out));

  // ── rejoin ──
  console.log("rejoin");
  b.clear();
  a.send({ type: "session_rejoin" });
  check("A rejoins", !!(await a.wait("session_started", 10000)));
  const back = await b.waitWhere((m) => m.type === "participants" && m.event === "joined", 5000);
  check("…as a join: B is told, and not displaced", back && back.who === "Phone" && !b.msgs.some((m) => m.type === "session_left"), JSON.stringify(b.msgs));

  // ── end, by a participant who is not the lead ──
  console.log("end");
  b.clear();
  a.send({ type: "session_stop" });
  const ended = await b.wait("session_left", 5000);
  check("A ends it for everyone; B is told who", ended && ended.reason === "ended" && ended.by === "Phone", JSON.stringify(ended));
  check("…and A that it stopped", !!(await a.wait("session_stopped", 5000)));
  await sleep(4000);

  // ── refusals ──
  console.log("refusal");
  a.clear();
  a.send({ type: "session_start", config: config(640, 360) });
  check("A starts another", !!(await a.wait("session_started", 30000)));
  b.clear();
  b.send({ type: "link_report", down_kbps: 900, rtt_ms: 40, intent: "join" });
  const slow = await b.wait("link_verdict");
  check("a link below the floor: the speed test refuses, and says why", slow && slow.verdict.level === "refuse" && /0\.9 Mbps/.test(slow.verdict.reasons[0]),
    JSON.stringify(slow && slow.verdict));
  b.send({ type: "session_join" });
  const no = await b.wait("session_error", 5000);
  check("…and the join it was for is refused", no && /speed test/.test(no.message), JSON.stringify(no));
  const lab = new Client("Lab", { instance: `${RID}:1`, room: true });
  check("a legacy device's joined room comes up", !!(await lab.up()), JSON.stringify(lab.msgs));
  lab.send({ type: "session_join" });
  const nog = await lab.wait("session_error", 5000);
  check("…but with no `join` grant it is refused, told how to allow it", nog && /wado allow Lab join/.test(nog.message), JSON.stringify(nog));

  // ── a second session: a sibling daemon ──
  console.log("sibling");
  a.clear();
  a.send({ type: "desk_request" });
  const where = await a.waitWhere((m) => m.type === "session_elsewhere" || m.type === "desks_full" || m.type === "session_error", 20000);
  check("Start new on a busy daemon starts a sibling and points there", where && where.type === "session_elsewhere" && where.instance !== `${RID}:1`,
    JSON.stringify(where));
  const sib = siblings();
  check("…a daemon of its own", sib.length === 1, JSON.stringify(sib));
  if (where && where.instance) {
    // As the page does: its link moves — one link per browser at a time.
    a.leave();
    await sleep(500);
    const c = new Client("Phone", { instance: where.instance, key: "Phone-key" });
    const up = await c.up(15000);
    check("A lands on the sibling", !!up && c.msgs.some((m) => m.type === "join_accepted" && m.instance_id === where.instance), JSON.stringify(c.msgs.slice(0, 3)));
    c.send({ type: "session_start", config: config(640, 360) });
    check("…and starts its own session there", !!(await c.wait("session_started", 30000)));
    await sleep(4000); // its session file is written on a 3 s heartbeat
    c.send({ type: "sessions_request" });
    const both = await c.wait("sessions");
    check("both sessions are listed, side by side", both && both.sessions.length === 2, JSON.stringify(both && both.sessions.map((s) => s.instance)));
    c.send({ type: "session_stop" });
    await c.wait("session_stopped", 5000);
    c.leave();
    let gone = false;
    for (let i = 0; i < 90 && !gone; i++) { await sleep(1000); gone = siblings().length === 0; }
    check("the sibling exits once its session ends", gone);
  }
} catch (e) {
  failures++;
  console.log("FAIL  threw: " + (e && e.stack || e));
} finally {
  if (process.env.E2E_LOG && d1) console.log(d1.out);
  cleanup();
  // Let each daemon stop its session (its apps, its bus) before this process goes.
  for (let i = 0; i < 50 && (procs.size || siblings().length); i++) await sleep(100);
}
console.log(failures ? `\n${failures} failed` : "\nall passed");
process.exit(failures ? 1 : 0);
