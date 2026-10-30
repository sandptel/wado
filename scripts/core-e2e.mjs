// The Rust client core (crates/client-core) against the real binaries: a local relay and a release
// daemon, with a throwaway config and runtime dir. Mints two pairing links with `wado qr` and runs
// crates/client-core/tests/live.rs against them:
//
//   cargo build --release -p wado -p wado-relay
//   node scripts/core-e2e.mjs        (run with the sandbox off)
//
// Covers: QR pairing on the seat then leaving it (4001); the control room opening and reading the
// host state; a spent code refused; a second device pairing (taking the seat) while the first
// holds its control room — so the control room took no seat.
//
// Kills only processes it started, by pid.

import { spawn, spawnSync, execFileSync } from "node:child_process";
import { mkdtempSync, mkdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const ROOT = new URL("..", import.meta.url).pathname;
const BIN = join(ROOT, process.env.WADO_BIN || "target/release");
const PORT = 4991;
const RELAY = `ws://127.0.0.1:${PORT}`;
const RID = "999000555";
const T = mkdtempSync(join(tmpdir(), "wado-core-e2e-"));
const CFG = join(T, "cfg");
const RUN = join(T, "run");
mkdirSync(join(CFG, "wado"), { recursive: true });
mkdirSync(RUN, { recursive: true, mode: 0o700 });
const ENV = { XDG_CONFIG_HOME: CFG, XDG_RUNTIME_DIR: RUN };
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const procs = new Set();
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
function cleanup() {
  for (const p of procs) try { p.kill("SIGTERM"); } catch {}
}
// A pairing link for the local relay, exactly as `wado qr` prints it.
const mint = () => {
  const out = execFileSync(join(BIN, "wado"), ["qr", "--relay", `http://127.0.0.1:${PORT}`, "--id", RID], { env: { ...process.env, ...ENV } })
    .toString().replace(/\x1b\[[0-9;]*m/g, "");
  return out.match(/open: (\S+)/)[1];
};

let code = 1;
try {
  const relay = start(join(BIN, "wado-relay"), ["--bind", "127.0.0.1:" + PORT]);
  await sleep(400);
  const d = start(join(BIN, "wado"), ["daemon"], { WADO_RELAY_URL: RELAY, WADO_REMOTE_ID: RID, WADO_INSTANCE: "1", RUST_LOG: "info" });
  if (!(await waitFor(d, /registered — Remote ID/))) throw new Error("the daemon did not register:\n" + d.out.slice(-800) + relay.out.slice(-400));
  const link = mint();
  const link2 = mint();
  console.log("minted:", link.replace(/pair=[^&]+/, "pair=…"));
  const r = spawnSync("cargo", ["test", "-p", "wado-client-core", "--test", "live", "--", "--ignored", "--nocapture", "--test-threads=1"], {
    cwd: ROOT, stdio: "inherit",
    env: { ...process.env, WADO_E2E_LINK: link, WADO_E2E_LINK2: link2, WADO_E2E_CFG: CFG },
  });
  code = r.status ?? 1;
  if (code !== 0) console.log("daemon log tail:\n" + d.out.slice(-2000));
} catch (e) {
  console.error(e.message || e);
} finally {
  cleanup();
  await sleep(300);
  process.exit(code);
}
