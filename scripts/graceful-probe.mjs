#!/usr/bin/env node
// graceful-probe — a headless viewer that proves a session outlives its viewer.
//
// It speaks the relay wire protocol and nothing else: no WebRTC, no browser. That is the
// point. The question "does a dropped socket kill the desktop?" is answered entirely on the
// signalling plane, and answering it without a phone is what makes it a check rather than a
// field report.
//
// What it does, in order:
//   1. join  → session_start        expect session_started
//   2. session_launch               expect the app's pid to appear under the daemon
//   3. hard-close the WebSocket     the relay sends peer_disconnected
//   4. wait, then re-join           expect session_alive  ← the whole test
//   5. session_rejoin               expect session_started, same session
//   6. count the app pids again     expect the identical set
//   7. session_stop                 leave nothing behind
//
// Usage: node scripts/graceful-probe.mjs [ws://127.0.0.1:4000] [872990894]
//
// Exit 0 on pass, 1 on fail, and it says which step failed and what it saw instead.

import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";

const RELAY = process.argv[2] || "ws://127.0.0.1:4000";
const ID = (process.argv[3] || "872990894").replace(/[\s-]/g, "");
const GAP_MS = Number(process.env.GAP_MS || 5000);

const CONFIG = {
  width: 1280, height: 720, fps: 60, scale: 1.0, quality: "balanced",
};
// Something that stays up and is trivially greppable. `sleep` is a child of the session's
// process group, which is exactly the thing `stop_session` kills — so its survival is the
// measurement, not a proxy for it.
// A unique sleep duration is the whole trick: it makes an ordinary `sleep` greppable without
// needing `exec -a`, which is a bashism and `proc::spawn` runs commands through `sh`.
const MARKER = String(9000 + Math.floor(Math.random() * 900));
// Wrapped in a real Wayland client when one is to hand, because the CPU A/B below is
// meaningless without a window: rendering an empty scene is nearly free whether it is paused or
// not, so a bare `sleep` measures the saving as far smaller than it is. The marker still works —
// `kitty -e sleep N` matches the same suffix, and the sleep is in the same process group.
const TERM = process.env.WADO_PROBE_TERM ?? "kitty";
const LAUNCH = TERM ? `${TERM} -e sleep ${MARKER}` : `sleep ${MARKER}`;

let failed = null;
const step = (n, msg) => console.log(`  ${n}. ${msg}`);
const fail = (msg) => { if (!failed) failed = msg; console.log(`  ✖ ${msg}`); };

// pids of the marker process. Exact-args match, never a loose pattern — a loose one matches
// this script's own command line, which has produced two false positives in this project.
function markerPids() {
  try {
    const out = execFileSync("ps", ["-eo", "pid=,args="], { encoding: "utf8" });
    return out.split("\n")
      .filter((l) => / sleep MARKER$/.test(l.replace(MARKER, "MARKER")))
      .map((l) => l.trim().split(/\s+/)[0])
      .sort();
  } catch { return []; }
}

// Daemon CPU over a window, as a percentage of one core. utime+stime from /proc/<pid>/stat,
// which is the only reading here that is not a self-report.
//
// This is the A/B that says whether pausing the render tick was worth doing, and the probe can
// take both halves without a browser: between `session_start` and the disconnect the session is
// marked attached with no viewer actually receiving — which is exactly the old behaviour, a
// session rendering and encoding for nobody. After the disconnect it is paused. Same daemon,
// same session, seconds apart.
function daemonPid() {
  try { return execFileSync("pgrep", ["-x", "wado"], { encoding: "utf8" }).trim().split("\n")[0]; }
  catch { return null; }
}
function cpuTicks(pid) {
  try {
    const f = readFileSync(`/proc/${pid}/stat`, "utf8").split(" ");
    return Number(f[13]) + Number(f[14]);      // utime + stime, in clock ticks
  } catch { return null; }
}
async function cpuPercent(pid, ms) {
  const a = cpuTicks(pid);
  if (a === null) return null;
  await sleep(ms);
  const b = cpuTicks(pid);
  if (b === null) return null;
  return ((b - a) / 100) / (ms / 1000) * 100;   // USER_HZ is 100 on Linux
}

// One relay connection. Resolves with a tiny handle; every message is pushed to `seen` and
// `expect` waits for one by type with its own timeout — per-request, never per-connection,
// which is the failure mode the branch exists to remove.
function dial() {
  return new Promise((resolve, reject) => {
    const ws = new WebSocket(`${RELAY}/join/${ID}`);
    const waiters = [];
    const seen = [];
    const t = setTimeout(() => reject(new Error("relay did not accept the join in 10 s")), 10000);
    ws.onmessage = (ev) => {
      let m; try { m = JSON.parse(ev.data); } catch { return; }
      if (m.type === "ping") { ws.send(JSON.stringify({ type: "pong" })); return; }
      seen.push(m);
      for (let i = waiters.length - 1; i >= 0; i--) {
        if (waiters[i].types.includes(m.type)) { waiters[i].resolve(m); waiters.splice(i, 1); }
      }
      if (m.type === "join_accepted") { clearTimeout(t); resolve(handle); }
      if (m.type === "join_denied") { clearTimeout(t); reject(new Error("join denied: " + m.reason)); }
    };
    ws.onerror = () => { clearTimeout(t); reject(new Error("WebSocket error — is the relay up?")); };
    const handle = {
      send: (o) => ws.send(JSON.stringify(o)),
      close: () => ws.close(),
      // Terminate without a close frame, the way a phone going into a tunnel does.
      kill: () => { try { ws.close(3000, "yanked"); } catch {} },
      expect: (types, ms = 8000) => new Promise((res, rej) => {
        const hit = seen.find((m) => types.includes(m.type));
        if (hit) return res(hit);
        const w = { types, resolve: res };
        waiters.push(w);
        setTimeout(() => {
          const i = waiters.indexOf(w);
          if (i >= 0) { waiters.splice(i, 1); rej(new Error(`timed out waiting for ${types}`)); }
        }, ms);
      }),
    };
  });
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function main() {
  console.log(`graceful-probe → ${RELAY} id=${ID}\n`);

  // ── 1. start ──────────────────────────────────────────────────────────────
  let a = await dial();
  step(1, "joined; asking for a session");
  a.send({ type: "session_start", config: CONFIG });
  let m = await a.expect(["session_started", "session_alive", "session_error"], 15000);
  if (m.type === "session_alive") {
    step(1, "a session was already running — dropping it so the probe starts clean");
    a.send({ type: "session_stop" });
    await a.expect(["session_stopped"]);
    a.send({ type: "session_start", config: CONFIG });
    m = await a.expect(["session_started", "session_error"], 15000);
  }
  if (m.type !== "session_started") return fail(`session_start said ${m.type}: ${m.message || ""}`);
  step(1, `session started — encoder ${m.info?.encoder?.mode || "?"}`);

  // ── 2. launch ─────────────────────────────────────────────────────────────
  a.send({ type: "session_launch", command: LAUNCH });
  await a.expect(["session_launched"]);
  await sleep(1500);
  const before = markerPids();
  if (!before.length) return fail("the launched marker process never appeared — cannot measure survival");
  step(2, `launched sleep ${MARKER} — pids [${before.join(",")}]`);

  const pid = daemonPid();
  const busy = pid ? await cpuPercent(pid, 4000) : null;
  if (busy !== null) step(2, `daemon CPU while rendering for nobody: ${busy.toFixed(1)}% of a core`);

  // ── 3. yank the socket ────────────────────────────────────────────────────
  a.kill();
  step(3, `socket closed without a session_stop; waiting ${GAP_MS} ms`);
  await sleep(1000);                            // let the detach land before sampling
  const idle = pid ? await cpuPercent(pid, 4000) : null;
  if (idle !== null) {
    step(3, `daemon CPU while detached: ${idle.toFixed(1)}% of a core`
      + (busy !== null ? `  (was ${busy.toFixed(1)}%)` : ""));
    if (busy !== null && idle > busy) fail(`pausing the render tick cost CPU instead of saving it`);
  }
  await sleep(Math.max(0, GAP_MS - 5000));

  // ── 4. the measurement ────────────────────────────────────────────────────
  const during = markerPids();
  if (during.join() !== before.join()) {
    fail(`the applications did not survive the disconnect: [${before.join(",")}] → [${during.join(",")}]`);
  } else {
    step(4, `applications survived — still [${during.join(",")}]`);
  }

  const b = await dial();
  b.send({ type: "session_start", config: CONFIG });
  const back = await b.expect(["session_alive", "session_started", "session_error"], 15000);
  if (back.type !== "session_alive") {
    fail(`after reconnecting the daemon said ${back.type} — the session did not survive`);
  } else {
    step(4, `the daemon still has the session: ${back.info?.encoder?.mode || "?"}`);
  }

  // ── 5-6. rejoin, and check nothing was recycled underneath ────────────────
  b.send({ type: "session_rejoin" });
  const re = await b.expect(["session_started", "session_error"], 10000);
  if (re.type !== "session_started") fail(`session_rejoin said ${re.type}: ${re.message || ""}`);
  else step(5, "rejoined the running session");

  const after = markerPids();
  if (after.join() !== before.join()) fail(`pids changed across the rejoin: [${before.join(",")}] → [${after.join(",")}]`);
  else step(6, `same applications after the rejoin — [${after.join(",")}]`);

  // ── 7. clean up ───────────────────────────────────────────────────────────
  b.send({ type: "session_stop" });
  await b.expect(["session_stopped"]);
  await sleep(1000);
  const gone = markerPids();
  if (gone.length) fail(`session_stop left ${gone.length} process(es) behind: [${gone.join(",")}]`);
  else step(7, "an explicit stop still kills everything — no leak");
  b.close();
}

main()
  .catch((e) => fail(e.message))
  .finally(() => {
    console.log(failed ? `\nFAIL — ${failed}` : "\nPASS — the session outlives its viewer");
    process.exit(failed ? 1 : 0);
  });
