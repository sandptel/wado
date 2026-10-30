// End-to-end check of F4's fallback lane and relay health (Decision Log 2026-10-27), against the
// real binaries: a relay, one daemon with its own HOME, a paired device speaking the envelope
// (scripts/lib/e2e-device.mjs) and the client's own bundle in headless Chrome
// (scripts/lib/bundle-page.mjs).
//
//   cargo build --release -p wado -p wado-relay
//   node scripts/lane-e2e.mjs                      (needs a GPU and Chrome; sandbox off)
//
//   caps        the relay announces `lane`, and /health says its numbers
//   down        the daemon paces what it sends at ≤ the cap, and an answer overtakes a 1 MB burst
//   up          the relay holds a device to the cap: what it sends past its burst waits
//   files       on the lane: hello, a listing, a file ≤ 1 MB with its hash; a bigger file and a
//               thumbnail are refused as the lane's (`lane: true`), in the network's words
//   client      js/files_link.js falls back to the lane when the session has no direct path, and
//               js/lane_hint.js raises the files hint
//   pad         a ping offering compact frames gets a pong accepting them; a compact frame is read
//   status      `wado relay status`: up while it is, exit 1 and the daemon's record says lost when
//               the relay goes, up again when it is back
//
// Kills only what it started, by pid.

import { spawn, execFileSync, spawnSync } from "node:child_process";
import { createHash, randomBytes } from "node:crypto";
import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { deviceFor, secure } from "./lib/e2e-device.mjs";
import { bundlePage, pairPage } from "./lib/bundle-page.mjs";

const ROOT = new URL("..", import.meta.url).pathname;
const BIN = join(ROOT, process.env.WADO_BIN || "target/release");
const PORT = 4987, HTTP = 4988, CDP = 9373;
const RELAY = `http://127.0.0.1:${PORT}`;
const RID = "999000333";
const T = mkdtempSync(join(tmpdir(), "wado-lane-e2e-"));
const HOME = join(T, "home");
const ENV = {
  HOME, XDG_CONFIG_HOME: join(HOME, ".config"), XDG_DATA_HOME: join(HOME, ".local/share"),
  XDG_STATE_HOME: join(HOME, ".local/state"), XDG_CACHE_HOME: join(HOME, ".cache"), XDG_RUNTIME_DIR: join(T, "run"),
};
for (const d of ["docs", "Pictures", ".config/wado"]) mkdirSync(join(HOME, d), { recursive: true });
mkdirSync(ENV.XDG_RUNTIME_DIR, { recursive: true, mode: 0o700 });
const small = randomBytes(200 * 1024);
writeFileSync(join(HOME, "docs/small.bin"), small);
const mid = randomBytes(900 * 1024);
writeFileSync(join(HOME, "docs/mid.bin"), mid);
writeFileSync(join(HOME, "docs/big.bin"), randomBytes(2 << 20));
writeFileSync(join(HOME, "Pictures/p.png"), randomBytes(1000));

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const sha = (b) => createHash("sha256").update(b).digest("hex");
const procs = new Set();
let failures = 0;
const check = (name, ok, detail = "") => {
  if (!ok) failures++;
  console.log(`${ok ? "  ok  " : "FAIL  "}${name}${ok || !detail ? "" : "\n        " + String(detail).slice(0, 900)}`);
};
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
const cleanup = () => { for (const p of procs) try { p.kill("SIGTERM"); } catch {} };
for (const sig of ["SIGTERM", "SIGINT", "SIGHUP"]) process.on(sig, () => { cleanup(); process.exit(1); });
async function waitFor(p, re, ms = 20000) {
  const t0 = Date.now();
  while (Date.now() - t0 < ms) { if (re.test(p.out)) return true; await sleep(100); }
  return false;
}
const wado = (...args) => spawnSync(join(BIN, "wado"), args, { env: { ...process.env, ...ENV }, encoding: "utf8" });
const mint = () => {
  const out = execFileSync(join(BIN, "wado"), ["qr", "--relay", RELAY, "--id", RID], { env: { ...process.env, ...ENV } })
    .toString().replace(/\x1b\[[0-9;]*m/g, "");
  const q = new URLSearchParams(out.match(/open: \S+\?(\S+)/)[1]);
  return { pair: q.get("pair"), hk: q.get("hk") };
};

// A paired device on its own relay socket: every frame it receives, sized as it came off the wire.
class Device {
  constructor(key, qr) {
    this.dev = deviceFor(key);
    if (qr) {
      this.dev.store.setItem("wado.pair", qr.pair);
      this.dev.store.setItem("wado.hostpins", JSON.stringify({ [RID]: { pin: qr.hk, from: "qr" } }));
    }
    this.msgs = [];
    this.wire = []; // [ms, bytes, type]
    const q = new URLSearchParams({ client: key, name: key });
    if (qr) q.set("pair", "1");
    this.ws = new WebSocket(`ws://127.0.0.1:${PORT}/join/${RID}?${q}`);
    this.ws.addEventListener("message", (e) => this.wire.push([performance.now(), e.data.length]));
    this.send = secure(this.ws, this.dev, RID, (m) => { m.__at = performance.now(); this.msgs.push(m); });
  }
  async waitWhere(pred, ms = 10000) {
    const t0 = Date.now();
    while (Date.now() - t0 < ms) { const m = this.msgs.find(pred); if (m) return m; await sleep(20); }
    return null;
  }
  wait(type, ms) { return this.waitWhere((m) => m.type === type, ms); }
  clear() { this.msgs.length = 0; this.wire.length = 0; }
  close() { try { this.ws.close(4001, "leave"); } catch {} }
}

let relay, d, page;
try {
  relay = start(join(BIN, "wado-relay"), ["--bind", "127.0.0.1:" + PORT]);
  await sleep(400);
  d = start(join(BIN, "wado"), ["daemon"], { WADO_RELAY_URL: `ws://127.0.0.1:${PORT}`, WADO_REMOTE_ID: RID, WADO_INSTANCE: "1", WADO_UDP_SLICE: "15" });
  check("the daemon registers", await waitFor(d, /registered — Remote ID/), d.out.slice(-600));

  // ── caps ──
  console.log("caps");
  const health = await (await fetch(`${RELAY}/health`)).json();
  check("/health says the lane's numbers", health.lane && health.lane.bytes_per_sec === 65536 && health.lane.burst === 786432, JSON.stringify(health));
  const a = new Device("lane-phone", mint());
  check("a device pairs by code and comes up", !!(await a.wait("__up", 15000)), JSON.stringify(a.msgs.slice(0, 4)));
  const acc = a.msgs.find((m) => m.type === "join_accepted");
  check("join_accepted announces `lane`", acc && acc.caps.includes("lane"), JSON.stringify(acc && acc.caps));

  check("wado files grant", wado("files", "grant", "lane-phone", "rw").status === 0);
  let fid = 0;
  const filesReq = async (op, args = {}, ms = 30000) => {
    const id = ++fid;
    a.send({ type: "files_in", text: JSON.stringify({ id, op, ...args }) });
    const m = await a.waitWhere((x) => x.type === "files_out" && JSON.parse(x.text).id === id, ms);
    return m ? JSON.parse(m.text) : null;
  };

  // ── down: the daemon paces a transfer on the lane; an answer overtakes it ──
  console.log("down");
  await sleep(2500);
  a.clear();
  const t0 = performance.now();
  const midId = ++fid;
  a.send({ type: "files_in", text: JSON.stringify({ id: midId, op: "get", path: join(HOME, "docs/mid.bin") }) });
  await sleep(2500);
  const asked = performance.now();
  a.send({ type: "host_net_request" });
  const net = await a.wait("host_net", 10000);
  const midEnd = await a.waitWhere((x) => x.type === "files_out" && JSON.parse(x.text).id === midId && JSON.parse(x.text).done, 40000);
  const midGot = Buffer.concat(a.msgs.filter((x) => x.type === "files_chunk" && x.id === midId).map((x) => Buffer.from(x.b, "base64")));
  const wire = a.wire.filter(([t]) => t >= t0);
  const bytes = wire.reduce((s, [, n]) => s + n, 0);
  const secs = (wire.at(-1)[0] - t0) / 1000;
  const rate = (bytes - 160 * 1024) / secs; // past the daemon's own burst
  console.log(`     900 KB file on the lane: ${(bytes / 1024).toFixed(0)} KB on the wire in ${secs.toFixed(1)} s ≈ ${(rate / 1024).toFixed(1)} KB/s after the burst`);
  check("the file arrives whole on the lane", midEnd && sha(midGot) === sha(mid), `${midGot.length} bytes`);
  check("…paced under the relay's 64 KB/s", rate <= 66 * 1024 && secs > 10, `${(rate / 1024).toFixed(1)} KB/s over ${secs.toFixed(1)} s`);
  const late = net ? Math.round(net.__at - asked) : null;
  console.log(`     host_net answered ${late} ms after asking, in the middle of the transfer`);
  check("an answer overtakes the transfer (no head-of-line wait)", net && late < 1500, `${late} ms`);

  // ── the speed test measures the path, not the lane ──
  console.log("speed test");
  await sleep(5000);
  a.clear();
  const s0 = performance.now();
  a.send({ type: "link_test", bytes: 384 * 1024 });
  const sDone = await a.wait("link_test_done", 20000);
  const sMs = Math.round(performance.now() - s0);
  const sKbps = Math.round((384 * 1024 * 8) / sMs);
  console.log(`     384 KB speed test in ${sMs} ms ≈ ${sKbps} kbps`);
  check("F6's speed test is not held to 64 KB/s (it would refuse every join)", sDone && sKbps > 1500 * 2, `${sKbps} kbps`);

  // ── up: the relay holds the device to the cap ──
  console.log("up");
  await sleep(4000);
  a.clear();
  const line = "x".repeat(16 * 1024);
  const u0 = performance.now();
  for (let i = 0; i < 70; i++) a.send({ type: "client_log", line });
  a.send({ type: "host_net_request" });
  const upNet = await a.wait("host_net", 40000);
  const upMs = upNet ? Math.round(upNet.__at - u0) : null;
  console.log(`     70 × 16 KB sent at once, then a question: answered after ${upMs} ms`);
  check("the relay paces a device's flood past its burst (it waits; nothing is dropped)", upNet && upMs > 5000, `${upMs} ms`);

  // ── files on the lane ──
  console.log("files");
  a.clear();
  const hello = await filesReq("hello");
  check("hello on the lane", hello && hello.ok && hello.access === "rw", JSON.stringify(hello));
  const list = await filesReq("list", { path: join(HOME, "docs") });
  check("a listing on the lane", list && list.ok && JSON.stringify(list).includes("small.bin"), JSON.stringify(list).slice(0, 300));
  const getId = ++fid;
  a.send({ type: "files_in", text: JSON.stringify({ id: getId, op: "get", path: join(HOME, "docs/small.bin") }) });
  const g0 = performance.now();
  const end = await a.waitWhere((x) => x.type === "files_out" && JSON.parse(x.text).id === getId && JSON.parse(x.text).done, 30000);
  const got = Buffer.concat(a.msgs.filter((x) => x.type === "files_chunk" && x.id === getId).map((x) => Buffer.from(x.b, "base64")));
  console.log(`     200 KB file on the lane: ${Math.round(performance.now() - g0)} ms`);
  check("a file ≤ 1 MB arrives whole, its hash checked", end && got.length === small.length && JSON.parse(end.text).sha256 === sha(small) && sha(got) === sha(small),
    `${got.length} bytes, ${end && end.text}`);
  const bigR = await filesReq("get", { path: join(HOME, "docs/big.bin") });
  check("a bigger file is refused as the lane's, in the network's words", bigR && bigR.lane === true && /not wado/.test(bigR.err) && /2\.0 MB/.test(bigR.err), JSON.stringify(bigR));
  const th = await filesReq("thumb", { path: join(HOME, "Pictures/p.png") });
  check("a thumbnail waits for a direct path", th && th.lane === true && /direct connection/.test(th.err), JSON.stringify(th));

  // ── pad: compact frames negotiated ──
  console.log("pad");
  check("wado allow … input", wado("allow", "lane-phone", "input").status === 0);
  a.clear();
  a.send({ type: "pad_in", msg: { k: "ping", t: 7, bin: true } });
  const pong = await a.waitWhere((m) => m.type === "pad_out" && m.msg.k === "pong", 8000);
  check("a ping offering compact frames gets a pong accepting them", pong && pong.msg.bin === true, JSON.stringify(a.msgs.slice(-3)));
  // An empty frame — every finger up — so nothing moves on this machine's desktop.
  a.send({ type: "pad_in", msg: { k: "bin", b: Buffer.from([0, 0, 0, 1, 0, 0, 0, 9]).toString("base64") } });
  a.send({ type: "pad_in", msg: { k: "bye" } });
  await sleep(800);
  check("a compact frame is read (no unreadable-frame warning, no unknown message)",
    !/unreadable compact frame|could not understand/.test(d.out) && !a.msgs.some((m) => m.type === "session_error"), d.out.split("\n").filter((l) => /pad/.test(l)).join("\n"));
  a.close();
  await sleep(500);

  // ── the client's own files link falls back to the lane ──
  console.log("client");
  page = await bundlePage({ dir: T, cdpPort: CDP, httpPort: HTTP, start });
  const qr = mint();
  await pairPage(page, RID, qr);
  const pageKey = await page.ev("window.__wado.clientKey");
  await page.ev(`window.__wado.relayDial(${JSON.stringify(RELAY)}, ${JSON.stringify(RID)})`);
  let up = false;
  for (let i = 0; i < 100 && !up; i++) { await sleep(100); up = await page.ev("window.__wado.relayUp"); }
  check("the page's link comes up", up);
  const granted = wado("files", "grant", pageKey.slice(0, 12), "rw");
  check("wado files grant (browser)", granted.status === 0, granted.stdout + granted.stderr);
  // A session with no direct path, as the link badge would say.
  const info = await page.ev(`(async () => { const W = window.__wado; W.linkNow = Object.assign({}, W.linkNow, { state: "relayed" });
    try { const i = await W.files.connect(); return { access: i.access, lane: W.files.lane }; } catch (e) { return { err: String(e.message || e) }; } })()`);
  check("files_link.js takes the lane when the session is relayed", info && info.lane === true && info.access === "rw", JSON.stringify(info));
  const hint = await page.ev(`window.__ev.filter(([, m]) => m.type === "laneHint").map(([, m]) => m)`);
  check("…and lane_hint.js raises the files hint", hint.some((m) => m.spot === "files" && m.on), JSON.stringify(hint));
  const pageGet = await page.ev(`(async () => {
    const F = window.__wado.files, parts = [];
    const end = await new Promise((ok) => F.stream("get", { path: ${JSON.stringify(join(HOME, "docs/small.bin"))} }, {
      msg: (m) => { if (m.done || m.err) { ok(m); return true; } return false; }, bytes: (u8) => parts.push(u8.slice()) }));
    const all = new Uint8Array(parts.reduce((s, p) => s + p.length, 0));
    let o = 0; for (const p of parts) { all.set(p, o); o += p.length; }
    const h = [...new Uint8Array(await crypto.subtle.digest("SHA-256", all))].map((b) => b.toString(16).padStart(2, "0")).join("");
    return { n: all.length, sha: h, said: end.sha256 || end.err };
  })()`);
  check("a download through the page on the lane is whole", pageGet.n === small.length && pageGet.sha === sha(small) && pageGet.said === sha(small), JSON.stringify(pageGet));
  const refused = await page.ev(`window.__wado.files.req("thumb", { path: ${JSON.stringify(join(HOME, "Pictures/p.png"))} }).then(() => "ok", (e) => String(e.message))`);
  const hint2 = await page.ev(`window.__ev.filter(([, m]) => m.type === "laneHint").map(([, m]) => m).at(-1)`);
  check("a lane refusal reaches the hint with the computer's words", /direct connection/.test(refused) && hint2 && /direct connection/.test(hint2.detail), JSON.stringify({ refused, hint2 }));
  page.close();
  page = null;

  // ── relay status ──
  console.log("status");
  let st = wado("relay", "status", "--json");
  let js = JSON.parse(st.stdout || "{}");
  check("wado relay status: reachable, the daemon connected, exit 0", st.status === 0 && js.reachable && js.daemons.length === 1 && js.daemons[0].up && js.daemons[0].caps.includes("lane"),
    st.stdout + st.stderr);
  relay.kill("SIGTERM");
  await sleep(3000);
  st = wado("relay", "status", "--json");
  js = JSON.parse(st.stdout || "{}");
  check("relay gone: exit 1, unreachable, and the daemon's own record says it lost it", st.status === 1 && !js.reachable && js.daemons[0] && !js.daemons[0].up && js.daemons[0].outages === 1,
    st.stdout);
  const human = wado("relay", "status");
  check("…in the words the plan asked for", /wado's relay is unreachable — not your computer/.test(human.stdout), human.stdout);
  relay = start(join(BIN, "wado-relay"), ["--bind", "127.0.0.1:" + PORT]);
  let back = false;
  for (let i = 0; i < 40 && !back; i++) { await sleep(500); const s = JSON.parse(wado("relay", "status", "--json").stdout || "{}"); back = s.reachable && s.daemons[0] && s.daemons[0].up; }
  check("relay back: the daemon redials and says so", back);
} catch (e) {
  failures++;
  console.log("FAIL " + (e.stack || e));
  if (d) console.log(d.out.slice(-1500));
} finally {
  if (page) page.close();
  cleanup();
}
console.log(failures ? `\n${failures} failed` : "\nall lane checks passed");
process.exit(failures ? 1 : 0);
