// The Mac daemon end to end: a local relay, Wado.app (as `open` starts it, so its own privacy
// grants apply), a throwaway config, and crates/client-core/tests/mac_live.rs as the phone.
//
//   scripts/mac-app.sh && cargo build --release -p wado-relay
//   node scripts/mac-e2e.mjs
//
// Covers: pairing; platform "macos" and the capability states; outputs and the media card; the
// clipboard both ways through the real pasteboard; the touchpad moving the pointer (when Wado
// has Accessibility). Kills only what it started.
import { spawn, execFileSync } from "node:child_process";
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const ROOT = new URL("..", import.meta.url).pathname;
const APP = join(ROOT, "target/Wado.app");
const WADO = join(APP, "Contents/MacOS/wado");
const PORT = 4993;
const RELAY = `ws://127.0.0.1:${PORT}`;
const RID = "999000777";
const T = mkdtempSync(join(tmpdir(), "wado-mac-e2e-"));
const CFG = join(T, "cfg");
const LOG = join(T, "daemon.log");
mkdirSync(join(CFG, "wado"), { recursive: true });
const ENV = { XDG_CONFIG_HOME: CFG };
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

let relay;
let code = 1;
const daemonPid = () => {
  try {
    return execFileSync("/usr/bin/pgrep", ["-f", `^${WADO} daemon$`]).toString().trim().split("\n").filter(Boolean);
  } catch { return []; }
};
const before = new Set(daemonPid());
function cleanup() {
  try { relay?.kill("SIGTERM"); } catch {}
  for (const p of daemonPid()) if (!before.has(p)) try { process.kill(Number(p), "SIGTERM"); } catch {}
}
for (const sig of ["SIGTERM", "SIGINT", "SIGHUP"]) process.on(sig, () => { cleanup(); process.exit(1); });

try {
  if (!existsSync(WADO)) throw new Error("build Wado.app first: scripts/mac-app.sh");
  relay = spawn(join(ROOT, "target/release/wado-relay"), ["--bind", "127.0.0.1:" + PORT], { stdio: ["ignore", "pipe", "pipe"] });
  relay.out = "";
  relay.stdout.on("data", (d) => (relay.out += d));
  relay.stderr.on("data", (d) => (relay.out += d));
  await sleep(400);
  execFileSync("/usr/bin/open", ["-n", "-a", APP, "--stdout", LOG, "--stderr", LOG,
    "--env", `XDG_CONFIG_HOME=${CFG}`, "--env", `WADO_RELAY_URL=${RELAY}`, "--env", `WADO_REMOTE_ID=${RID}`,
    "--env", "WADO_INSTANCE=1", "--env", "RUST_LOG=info,wado=debug", "--args", "daemon"]);
  const t0 = Date.now();
  while (Date.now() - t0 < 20000) {
    if (existsSync(LOG) && /registered — Remote ID/.test(readFileSync(LOG, "utf8"))) break;
    await sleep(200);
  }
  if (!/registered — Remote ID/.test(existsSync(LOG) ? readFileSync(LOG, "utf8") : "")) throw new Error("Wado.app did not register: " + LOG);
  const link = execFileSync(WADO, ["qr", "--relay", `http://127.0.0.1:${PORT}`, "--id", RID], { env: { ...process.env, ...ENV } })
    .toString().replace(/\x1b\[[0-9;]*m/g, "").match(/open: (\S+)/)[1];
  const t = spawn("cargo", ["test", "-p", "wado-client-core", "--test", "mac_live", "--", "--ignored", "--nocapture"], {
    cwd: ROOT, stdio: "inherit",
    env: { ...process.env, WADO_E2E_LINK: link, WADO_E2E_CFG: CFG, WADO_E2E_WADO: WADO },
  });
  code = await new Promise((r) => t.on("exit", (c) => r(c ?? 1)));
  writeFileSync(join(T, "relay.log"), relay.out);
  console.log(`logs: ${LOG}, ${T}/relay.log`);
} catch (e) {
  console.error(e.message || e);
} finally {
  cleanup();
  await sleep(300);
  process.exit(code);
}
