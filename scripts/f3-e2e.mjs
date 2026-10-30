// End-to-end check of F3 — the passkey step-up, Claude from the couch, Web Push — against the
// real binaries, with a software passkey made by WebCrypto (scripts/lib/e2e-passkey.mjs).
//
//   node scripts/f3-e2e.mjs [target/debug]     (default target/release)
//
// One relay, one idle daemon (no session, so no GPU), a throwaway HOME, config and runtime dir:
//
//   passkey   no passkey → shells refused with how to add one; register; a fresh link is locked
//             again; unlock; the lock message locks; a replayed assertion is refused
//   claude    a hook event through `wado claude-event` becomes a card; a permission request blocks
//             the hook until Allow (behind the passkey) and Claude reads "allow"; Deny without it
//   hooks     `wado claude hooks install --yes` writes the hooks once; uninstall takes them out
//   push      the computer's VAPID key; a bad subscription is refused; kinds are kept per device

import { spawn, spawnSync } from "node:child_process";
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { deviceFor, secure } from "./lib/e2e-device.mjs";
import { authenticator, enrolAndUnlock, unlock } from "./lib/e2e-passkey.mjs";

const ROOT = new URL("..", import.meta.url).pathname;
const BIN = join(ROOT, process.argv[2] || "target/release");
const PORT = 4991;
const RELAY = `ws://127.0.0.1:${PORT}`;
const RID = "999000333";
const HOME = mkdtempSync(join(tmpdir(), "wado-f3-e2e-"));
const RUNTIME = join(HOME, "run");
mkdirSync(RUNTIME, { mode: 0o700 });
mkdirSync(join(HOME, ".config/wado"), { recursive: true });
// The phone is trusted (a line from before keys) and, as the first line, the owner — shells allowed.
writeFileSync(join(HOME, ".config/wado/trusted_clients"), "Phone-key\tPhone\n");
const ENV = { HOME, XDG_CONFIG_HOME: join(HOME, ".config"), XDG_RUNTIME_DIR: RUNTIME, XDG_STATE_HOME: join(HOME, ".state"), SHELL: "/bin/sh" };
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
    this.dev = deviceFor(name + "-key");
    this.send = secure(this.ws, this.dev, RID, (m) => this.msgs.push(m));
  }
  async waitWhere(pred, ms = 10000) {
    const t0 = Date.now();
    while (Date.now() - t0 < ms) {
      const i = this.msgs.findIndex(pred);
      if (i >= 0) return this.msgs.splice(i, 1)[0];
      await sleep(30);
    }
    return null;
  }
  wait(type, ms) { return this.waitWhere((m) => m.type === type, ms); }
  close() { this.dev.W.e2eClose(this.ws); }
}

// `wado claude-event` with a hook's JSON on stdin, as Claude Code runs it.
function hook(event, env = {}) {
  const p = spawn(join(BIN, "wado"), ["claude-event"], { env: { ...process.env, ...ENV, TMUX: "", ...env }, stdio: ["pipe", "pipe", "pipe"] });
  p.out = "";
  p.stdout.on("data", (d) => (p.out += d));
  p.done = new Promise((r) => p.on("exit", r));
  p.stdin.end(JSON.stringify(event));
  return p;
}

try {
  start("wado-relay", ["--bind", `127.0.0.1:${PORT}`], {});
  await sleep(400);
  start("wado", ["daemon"], { ...ENV, WADO_RELAY_URL: RELAY, WADO_REMOTE_ID: RID, WADO_INSTANCE: "e2e-f3", WADO_UDP_SLICE: "20" });

  console.log("passkey");
  const pk = await authenticator();
  let a = new Client("Phone");
  check("joined", !!(await a.wait("join_accepted", 20000)));
  a.send({ type: "pty_open", cols: 80, rows: 24 });
  const refused = await a.waitWhere((m) => m.type === "passkey_state", 8000);
  check("a shell is refused without a passkey — with the lock, not an error",
    refused && !refused.state.registered && !refused.state.unlocked, JSON.stringify(refused));
  a.send({ type: "passkey_begin", purpose: "unlock" });
  const none = await a.wait("passkey_state");
  check("…and unlocking says how to add one", none && /add one on the computer's page/.test(none.state.error), JSON.stringify(none));
  const on = await enrolAndUnlock(a.send, (p) => a.waitWhere(p), pk);
  check("a passkey registers and unlocks", on && on.state.registered && on.state.unlocked && on.state.idle_s === 900, JSON.stringify(on));
  a.send({ type: "pty_open", cols: 80, rows: 24 });
  const opened = await a.wait("pty_opened");
  check("now a shell opens", !!opened);
  a.send({ type: "passkey_lock" });
  const locked = await a.wait("passkey_state");
  check("lock (the app went to the background) locks", locked && !locked.state.unlocked);
  a.send({ type: "pty_input", id: opened && opened.id, data: "echo leak\n" });
  const typed = await a.waitWhere((m) => m.type === "passkey_state", 4000);
  check("typing into the shell while locked is refused", !!typed && !typed.state.unlocked);
  a.send({ type: "passkey_begin", purpose: "register" });
  const swap = await a.wait("passkey_state");
  check("a second passkey cannot be slipped in while locked", swap && /already has a passkey/.test(swap.state.error), JSON.stringify(swap));
  // A replay: the same assertion twice. The challenge is single-use.
  a.send({ type: "passkey_begin", purpose: "unlock" });
  const c = await a.wait("passkey_challenge");
  const as = await pk.assert(c.challenge, "https://wado.test");
  a.send({ type: "passkey_unlock", ...as });
  check("unlock with the passkey", !!(await a.waitWhere((m) => m.type === "passkey_state" && m.state.unlocked)));
  a.send({ type: "passkey_lock" });
  await a.wait("passkey_state");
  a.send({ type: "passkey_unlock", ...as });
  const replay = await a.wait("passkey_state");
  check("…and the same answer again is refused", replay && !replay.state.unlocked && /took too long/.test(replay.state.error), JSON.stringify(replay));
  a.close();
  await sleep(800);
  a = new Client("Phone");
  check("rejoined", !!(await a.wait("join_accepted", 20000)));
  a.send({ type: "passkey_get" });
  const fresh = await a.waitWhere((m) => m.type === "passkey_state" && m.state.registered);
  check("a new link starts locked", fresh && !fresh.state.unlocked, JSON.stringify(fresh));

  console.log("claude");
  a.send({ type: "claude_get" });
  const empty = await a.wait("claude_cards");
  check("cards answer, none yet, hooks not installed", empty && empty.state.sessions.length === 0 && !empty.state.hooks, JSON.stringify(empty));
  const h1 = hook({ session_id: "s1", hook_event_name: "UserPromptSubmit", cwd: "/home/me/api", prompt: "fix the flaky test" });
  await h1.done;
  const card = await a.waitWhere((m) => m.type === "claude_cards" && m.state.sessions.some((s) => s.id === "s1" && s.state === "working"));
  check("a hook event becomes a card: project, working, the prompt",
    card && card.state.sessions[0].project === "api" && card.state.sessions[0].message === "fix the flaky test", JSON.stringify(card));
  const h2 = hook({ session_id: "s1", hook_event_name: "PermissionRequest", cwd: "/home/me/api", tool_name: "Bash", tool_input: { command: "cargo test" } });
  const asking = await a.waitWhere((m) => m.type === "claude_cards" && m.state.sessions.some((s) => s.ask));
  const ask = asking && asking.state.sessions[0].ask;
  check("a permission request shows on the card", ask && ask.tool === "Bash" && ask.detail === "cargo test", JSON.stringify(asking));
  a.send({ type: "claude_answer", session: "s1", ask: ask && ask.id, allow: true });
  const gated = await a.wait("passkey_state", 4000);
  check("Allow needs the passkey", !!gated && !gated.state.unlocked);
  await unlock(a.send, (p) => a.waitWhere(p), pk);
  a.send({ type: "claude_answer", session: "s1", ask: ask && ask.id, allow: true });
  await Promise.race([h2.done, sleep(8000)]);
  let decided = {};
  try { decided = JSON.parse(h2.out); } catch (_) {}
  check("Allow reaches the waiting hook: Claude reads allow",
    decided.hookSpecificOutput && decided.hookSpecificOutput.decision.behavior === "allow", h2.out);
  const h3 = hook({ session_id: "s1", hook_event_name: "PermissionRequest", cwd: "/home/me/api", tool_name: "Edit", tool_input: { file_path: "/etc/passwd" } });
  const asking2 = await a.waitWhere((m) => m.type === "claude_cards" && m.state.sessions.some((s) => s.ask && s.ask.tool === "Edit"));
  a.send({ type: "passkey_lock" });
  await a.wait("passkey_state");
  a.send({ type: "claude_answer", session: "s1", ask: asking2 && asking2.state.sessions[0].ask.id, allow: false });
  await Promise.race([h3.done, sleep(8000)]);
  let denied = {};
  try { denied = JSON.parse(h3.out); } catch (_) {}
  check("Deny needs no passkey and reaches the hook", denied.hookSpecificOutput && denied.hookSpecificOutput.decision.behavior === "deny", h3.out);
  const h4 = hook({ session_id: "s1", hook_event_name: "Stop", last_assistant_message: "All tests pass." });
  await h4.done;
  check("Stop marks it done with its last words",
    !!(await a.waitWhere((m) => m.type === "claude_cards" && m.state.sessions.some((s) => s.state === "done" && s.message === "All tests pass."))));
  a.send({ type: "claude_new", dir: HOME });
  const newGate = await a.wait("passkey_state", 4000);
  check("a new session needs the passkey", !!newGate && !newGate.state.unlocked);

  console.log("hooks");
  const settings = join(HOME, ".claude/settings.json");
  mkdirSync(join(HOME, ".claude"), { recursive: true });
  writeFileSync(settings, JSON.stringify({ model: "opus", hooks: { Stop: [{ hooks: [{ type: "command", command: "theirs" }] }] } }, null, 2) + "\n");
  const inst = spawnSync(join(BIN, "wado"), ["claude", "hooks", "install", "--yes"], { env: { ...process.env, ...ENV }, encoding: "utf8" });
  const text = readFileSync(settings, "utf8");
  check("install shows a diff and writes the hooks", inst.status === 0 && /PermissionRequest/.test(inst.stdout) && (text.match(/claude-event/g) || []).length === 6, inst.stdout + inst.stderr);
  check("…keeping theirs and their key order", text.includes('"theirs"') && text.indexOf('"model"') < text.indexOf('"hooks"'));
  const again = spawnSync(join(BIN, "wado"), ["claude", "hooks", "install", "--yes"], { env: { ...process.env, ...ENV }, encoding: "utf8" });
  check("installing twice changes nothing", again.status === 0 && readFileSync(settings, "utf8") === text && /nothing to change/.test(again.stdout));
  a.send({ type: "claude_get" });
  check("the computer now says the hooks are in", !!(await a.waitWhere((m) => m.type === "claude_cards" && m.state.hooks)));
  spawnSync(join(BIN, "wado"), ["claude", "hooks", "uninstall", "--yes"], { env: { ...process.env, ...ENV }, encoding: "utf8" });
  const back = JSON.parse(readFileSync(settings, "utf8"));
  check("uninstall takes out only wado's", back.hooks.Stop.length === 1 && back.hooks.Stop[0].hooks[0].command === "theirs" && !back.hooks.PermissionRequest, JSON.stringify(back));

  console.log("push");
  a.send({ type: "push_get" });
  const info = await a.wait("push_info");
  check("the computer's VAPID key (65 bytes), not subscribed",
    info && Buffer.from(info.info.key, "base64url").length === 65 && !info.info.subscribed, JSON.stringify(info));
  a.send({ type: "push_subscribe", sub: { endpoint: "http://127.0.0.1/x", p256dh: "AA", auth: "AA" }, kinds: {} });
  const bad = await a.wait("push_info");
  check("a bad subscription is refused", bad && !bad.info.subscribed && /https/.test(bad.info.error), JSON.stringify(bad));
  const ua = await crypto.subtle.generateKey({ name: "ECDH", namedCurve: "P-256" }, true, ["deriveBits"]);
  const p256dh = Buffer.from(await crypto.subtle.exportKey("raw", ua.publicKey)).toString("base64url");
  const auth = Buffer.from(crypto.getRandomValues(new Uint8Array(16))).toString("base64url");
  a.send({ type: "push_subscribe", sub: { endpoint: "https://push.example.invalid/abc", p256dh, auth, kind: "web" }, kinds: { claude_done: false } });
  const sub = await a.wait("push_info");
  check("a subscription is kept, with its kinds", sub && sub.info.subscribed && sub.info.kinds.claude_permission && !sub.info.kinds.claude_done, JSON.stringify(sub));
  a.send({ type: "push_kinds_set", kinds: { claude_permission: false, claude_done: true, online: true, transfer: false } });
  const kinds = await a.wait("push_info");
  check("kinds change per device", kinds && !kinds.info.kinds.claude_permission && kinds.info.kinds.claude_done && !kinds.info.kinds.transfer);
  a.send({ type: "push_unsubscribe" });
  check("and it can be forgotten", !!(await a.waitWhere((m) => m.type === "push_info" && !m.info.subscribed)));
} finally {
  for (const p of procs) p.kill("SIGTERM");
}
console.log(failures ? `\n${failures} failed` : "\nall passed");
process.exit(failures ? 1 : 0);
