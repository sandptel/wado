// End-to-end check of daemon-owned shells and config-over-relay, against the real binaries.
//
//   node scripts/shells-e2e.mjs [target/debug]     (default target/release)
//
// One relay, one idle daemon (no session, so no GPU), a throwaway HOME and config dir. Covers:
//
//   shells     a shell opens, runs a command, and lists with its title
//   reattach   a viewer that drops and comes back gets the shell and its scrollback (as a replay)
//   ssh        hosts come from ~/.ssh/config, and an alias that is not there is refused
//   config     config_get answers; a live key set from the client lands and is pushed back
//   close      closing a tab ends the shell and the list says so

import { spawn } from "node:child_process";
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const ROOT = new URL("..", import.meta.url).pathname;
const BIN = join(ROOT, process.argv[2] || "target/release");
const PORT = 4988;
const RELAY = `ws://127.0.0.1:${PORT}`;
const RID = "999000222";
const HOME = mkdtempSync(join(tmpdir(), "wado-shells-e2e-"));
mkdirSync(join(HOME, ".ssh"));
writeFileSync(join(HOME, ".ssh/config"), "Host fakebox\n  HostName 192.0.2.1\nHost *.lan\n");
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const procs = [];
let failures = 0;
const check = (name, ok, detail = "") => {
  if (!ok) failures++;
  console.log(`${ok ? "  ok  " : "FAIL  "}${name}${ok || !detail ? "" : "\n        " + detail}`);
};
const start = (bin, args, env) => {
  const p = spawn(join(BIN, bin), args, { env: { ...process.env, ...env }, stdio: ["ignore", "pipe", "pipe"] });
  p.out = "";
  p.stdout.on("data", (d) => (p.out += d));
  p.stderr.on("data", (d) => (p.out += d));
  procs.push(p);
  return p;
};

class Client {
  constructor(name) {
    this.msgs = [];
    this.ws = new WebSocket(`${RELAY}/join/${RID}?${new URLSearchParams({ client: name + "-key", name })}`);
    this.ws.onmessage = (e) => {
      const m = JSON.parse(e.data);
      if (m.type === "ping") return this.send({ type: "pong" });
      this.msgs.push(m);
    };
  }
  send(m) { this.ws.send(JSON.stringify(m)); }
  async waitWhere(pred, ms = 10000) {
    const t0 = Date.now();
    while (Date.now() - t0 < ms) {
      const m = this.msgs.find(pred);
      if (m) return m;
      await sleep(50);
    }
    return null;
  }
  wait(type, ms) { return this.waitWhere((m) => m.type === type, ms); }
  output(id) { return this.msgs.filter((m) => m.type === "pty_output" && m.id === id).map((m) => m.data).join(""); }
  close() { this.ws.close(); }
}

try {
  start("wado-relay", ["--bind", `127.0.0.1:${PORT}`], {});
  await sleep(400);
  start("wado", [], {
    WADO_RELAY_URL: RELAY, WADO_REMOTE_ID: RID, WADO_INSTANCE: "1", WADO_UDP_SLICE: "20",
    HOME, XDG_CONFIG_HOME: join(HOME, ".config"), XDG_RUNTIME_DIR: HOME, SHELL: "/bin/sh",
  });

  console.log("shells");
  const a = new Client("Phone");
  check("joined", !!(await a.wait("join_accepted", 20000)));
  a.send({ type: "shells_request" });
  const first = await a.wait("shells");
  check("no shells yet, ssh hosts listed", first && first.shells.length === 0 && first.hosts.join() === "fakebox", JSON.stringify(first));
  a.send({ type: "pty_open", cols: 80, rows: 24 });
  const opened = await a.wait("pty_opened");
  check("a shell opens", !!opened);
  const id = opened && opened.id;
  a.send({ type: "pty_input", id, data: "echo wado-$((40+2))\n" });
  check("it runs a command", !!(await a.waitWhere(() => a.output(id).includes("wado-42"))), a.output(id).slice(-200));

  console.log("reattach");
  a.close();
  await sleep(800);
  const b = new Client("Phone");
  check("rejoined", !!(await b.wait("join_accepted", 20000)));
  b.send({ type: "shells_request" });
  const back = await b.wait("shells");
  check("the shell survived the drop", back && back.shells.some((s) => s.id === id && s.alive), JSON.stringify(back));
  const replay = await b.waitWhere((m) => m.type === "pty_output" && m.id === id && m.replay);
  check("…with its scrollback, as a replay", replay && replay.data.includes("wado-42"));

  console.log("ssh");
  b.send({ type: "pty_open", cols: 80, rows: 24, host: "evil -oProxyCommand=x" });
  const refused = await b.wait("session_error", 5000);
  check("an alias not in ~/.ssh/config is refused", refused && /not a Host/.test(refused.message), JSON.stringify(refused));

  console.log("config");
  b.send({ type: "config_get" });
  const st = await b.wait("config_state");
  check("config_get answers", !!st && st.state.limits.max_fps == null, JSON.stringify(st));
  check("…and the first device is the owner", st && st.state.owner === true);
  b.msgs.length = 0;
  b.send({ type: "config_set", key: "stream.max-fps", value: "90" });
  const pushed = await b.waitWhere((m) => m.type === "config_state" && m.state.limits.max_fps === 90);
  check("a live key set from the client lands and is pushed back", !!pushed, JSON.stringify(b.msgs.slice(-2)));
  check("…in ui.kdl, not config.kdl",
    readFileSync(join(HOME, ".config/wado/ui.kdl"), "utf8").includes("max-fps 90") &&
    !readFileSync(join(HOME, ".config/wado/config.kdl"), "utf8").includes("max-fps 90"));
  b.send({ type: "config_set", key: "shells.enabled", value: "false" });
  const unconfirmed = await b.wait("config_rejected", 5000);
  check("a privileged key needs confirming", unconfirmed && /confirm/.test(unconfirmed.message), JSON.stringify(unconfirmed));

  console.log("close");
  b.msgs.length = 0;
  b.send({ type: "pty_close", id });
  const after = await b.waitWhere((m) => m.type === "shells" && !m.shells.some((s) => s.id === id));
  check("closing a tab ends the shell", !!after);
} finally {
  for (const p of procs) p.kill("SIGTERM");
}
console.log(failures ? `\n${failures} failed` : "\nall passed");
process.exit(failures ? 1 : 0);
