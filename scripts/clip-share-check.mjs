// Runnable check for js/clip_share.js: the clipboard shared both ways by itself, against stubbed
// browsers — Chrome (writes while focused, reads once permitted), Safari (only inside a tap) and
// the native app — with echo suppression, the size cap, secrets, and the per-computer switches.
//   node scripts/clip-share-check.mjs
import { readFileSync } from "node:fs";
import { webcrypto, createHash } from "node:crypto";

const src = readFileSync(new URL("../crates/client/src/js/clip_share.js", import.meta.url), "utf8");
let failures = 0;
const check = (name, ok, got) => { if (!ok) failures++; console.log(`${ok ? "ok  " : "FAIL"} ${name}${ok ? "" : " — got " + JSON.stringify(got)}`); };
const tick = (ms = 15) => new Promise((r) => setTimeout(r, ms));
const hashOf = (t) => createHash("sha256").update(Buffer.from(t, "utf8")).digest("hex").slice(0, 32);

// One fresh page: `kind` is "chrome", "safari" or "native".
function page(kind, { perm = "granted" } = {}) {
  const listeners = {};
  const on = (t, f) => (listeners[t] = listeners[t] || []).push(f);
  const fire = (t, e = {}) => (listeners[t] || []).forEach((f) => f(e));
  const env = {
    clip: "", focused: true, visible: true, activation: false,
    writes: [], reads: 0, sent: [], emitted: [], nativeSets: [], appSent: [],
  };
  const store = new Map();
  const localStorage = { getItem: (k) => store.get(k) ?? null, setItem: (k, v) => store.set(k, String(v)) };
  const document = {
    get visibilityState() { return env.visible ? "visible" : "hidden"; },
    hasFocus: () => env.focused,
    addEventListener: on,
  };
  const window = { addEventListener: on, localStorage };
  const refuse = () => Promise.reject(Object.assign(new Error("NotAllowedError"), { name: "NotAllowedError" }));
  const navigator = {
    clipboard: {
      writeText(t) {
        if (kind === "safari" && !env.activation) return refuse();
        if (kind === "chrome" && !env.focused) return refuse();
        env.writes.push(t); env.clip = t; return Promise.resolve();
      },
      readText() {
        env.reads++;
        if (kind === "safari" && !env.activation) return refuse();
        if (kind === "chrome" && perm !== "granted" && !env.activation) return refuse();
        return Promise.resolve(env.clip);
      },
    },
    permissions: {
      query: ({ name }) => kind === "chrome" && name === "clipboard-read"
        ? Promise.resolve({ state: perm })
        : Promise.reject(new TypeError("unknown permission")),
    },
  };
  if (kind === "app") {
    window.wadoNative = { postMessage() {} };
  }
  if (kind === "native") {
    window.wadoNative = { clipboard: {
      set: (o) => { env.nativeSets.push(o); env.clip = o.data; env.sensitive = !!o.sensitive; return true; },
      get: () => ({ mime: "text/plain", data: env.clip, sensitive: !!env.sensitive }),
    } };
  }
  const W = {
    native: kind === "app" ? { send: (t, f) => env.appSent.push([t, f]) } : null,
    relayUp: true,
    relaySendMsg: (m) => env.sent.push(["laptop", m]),
    sideUp: (k) => k === "desk",
    sideDo: (k, m) => { env.sent.push([k, m]); return true; },
  };
  const emit = (m) => env.emitted.push(m);
  new Function("W", "emit", "window", "document", "navigator", "localStorage", "crypto", "ClipboardItem", src)(
    W, emit, window, document, navigator, localStorage, webcrypto, undefined,
  );
  const tap = (attrs) => {
    const el = {
      getAttribute: (k) => attrs[k] ?? null, setAttribute: () => {}, removeAttribute: () => {},
      closest() { return this; },
    };
    env.activation = true;
    fire("click", { target: el });
    env.activation = false;
  };
  const links = (list) => W.clipLinks(list);
  const said = () => env.emitted.filter((m) => m.type === "clipSaid").map((m) => m.text);
  const chip = () => [...env.emitted].reverse().find((m) => m.type === "clipChip")?.chip ?? null;
  const pushes = () => env.sent.filter(([, m]) => m.type === "clip_push");
  const watches = () => env.sent.filter(([, m]) => m.type === "clip_watch");
  const leave = () => { env.focused = false; env.visible = false; fire("visibilitychange"); };
  const come = async () => { env.focused = true; env.visible = true; fire("visibilitychange"); fire("focus"); await tick(40); };
  return { W, env, fire, tap, links, said, chip, pushes, watches, leave, come, sleep: tick };
}

const LAPTOP = { key: "laptop", name: "Laptop", selected: true, may: true, paired: true };
const DESK = { key: "desk", name: "Desk", selected: false, may: true, paired: true };
const now = (text, from = "desktop") => ({ hash: hashOf(text), mime: "text/plain;charset=utf-8", data: text, from, at_ms: 1 });

// ── defaults and the watch ─────────────────────────────────────────────────────────────────
{
  const p = page("chrome");
  p.links([LAPTOP, DESK, { key: "old", name: "Old", selected: false, may: true, paired: false }]);
  const w = p.watches();
  check("each paired computer with the grant is asked for every copy", w.length === 2 && w.every(([, m]) => m.live === true), w);
  const prefs = [...p.env.emitted].reverse().find((m) => m.type === "clipShare").prefs;
  check("on by default for a paired device, off for one paired before codes",
    prefs.laptop.pull && prefs.laptop.push && !prefs.old.pull && !prefs.old.push, prefs);
  p.W.clipSet("laptop", "pull", false);
  const last = p.watches().at(-1);
  check("turning the switch off tells that computer", last[0] === "laptop" && last[1].live === false, last);
  p.links([{ ...LAPTOP, may: false }]);
  const before = p.watches().length;
  p.W.clipShare.linkUp("laptop");
  check("no grant: nothing is asked for", p.watches().length === before, p.watches());
}

// ── computer → phone, Chrome ───────────────────────────────────────────────────────────────
{
  const p = page("chrome");
  p.links([LAPTOP]);
  p.W.clipShare.now("laptop", now("hello from the laptop"));
  await p.sleep();
  check("focused Chrome: written at once", p.env.writes[0] === "hello from the laptop", p.env.writes);
  check("and said, in words", p.said().includes("Copied to this phone from Laptop"), p.said());

  // Read back on the next focus: the same value, so nothing goes back to the computer.
  p.leave(); await p.come();
  check("echo: the copy just written is not sent back", p.pushes().length === 0, p.pushes());

  // Copied in the background: waits, and lands on the next focus.
  p.leave();
  p.W.clipShare.now("laptop", now("while away"));
  await p.sleep();
  check("in the background: not written yet", p.env.writes.length === 1, p.env.writes);
  await p.come();
  check("written on the next focus", p.env.writes.at(-1) === "while away", p.env.writes);
  check("…and not sent back either", p.pushes().length === 0, p.pushes());

  // The same copy arriving again (another link, a reconnect): nothing.
  p.W.clipShare.now("laptop", now("while away"));
  await p.sleep();
  check("a copy already here is not written twice", p.env.writes.length === 2, p.env.writes);

  // Switched off: copies are left alone.
  p.W.clipSet("laptop", "pull", false);
  p.W.clipShare.now("laptop", now("not wanted"));
  await p.sleep();
  check("switch off: not written", p.env.writes.length === 2, p.env.writes);
}

// ── phone → computer, Chrome ───────────────────────────────────────────────────────────────
{
  const p = page("chrome");
  p.links([LAPTOP, DESK]);
  p.W.clipSet("desk", "push", false);
  p.leave();
  p.env.clip = "copied on the phone";
  await p.come();
  const sent = p.pushes();
  check("permission granted: read on focus and sent", sent.length === 1 && sent[0][1].data === "copied on the phone", sent);
  check("only to the computers whose switch is on", sent.every(([k]) => k === "laptop"), sent);
  check("never marked sensitive by itself", sent[0][1].sensitive === false, sent[0]);
  p.W.clipShare.pushed("laptop", { hash: hashOf("copied on the phone") });
  check("the computer's ack is said", p.said().includes("Sent to Laptop"), p.said());

  p.leave(); await p.come();
  check("the same clipboard again is not sent again", p.pushes().length === 1, p.pushes());

  // The computer copies it back (another device sent the same): not written, not looped.
  p.W.clipShare.now("laptop", now("copied on the phone", "Tablet"));
  await p.sleep();
  check("the value already here is not rewritten", p.env.writes.length === 0, p.env.writes);

  // Over the cap.
  p.leave();
  p.env.clip = "x".repeat((1 << 20) + 1);
  await p.come();
  check("text over 1 MiB is not sent", p.pushes().length === 1, p.pushes().length);
}

// ── Chrome before the permission: a chip, never a prompt out of nowhere ────────────────────
{
  const p = page("chrome", { perm: "prompt" });
  p.links([LAPTOP]);
  p.leave(); await p.sleep(1600);
  p.env.clip = "phone text";
  await p.come();
  check("not yet permitted: nothing read on focus", p.env.reads === 0, p.env.reads);
  check("a Send chip is offered instead", p.chip()?.kind === "send", p.chip());
  p.tap({ "data-clip": "chip-send" });
  await p.sleep(40);
  check("the tap reads and sends", p.pushes().length === 1 && p.pushes()[0][1].data === "phone text", p.pushes());
}

// ── Safari: only inside a tap ──────────────────────────────────────────────────────────────
{
  const p = page("safari");
  p.links([LAPTOP]);
  p.W.clipShare.now("laptop", now("for safari"));
  await p.sleep();
  check("Safari refuses the write: a Copy chip, prepared", p.chip()?.kind === "copy" && p.chip()?.preview === "for safari", p.chip());
  p.tap({ "data-clip": "chip-copy" });
  await p.sleep();
  check("one tap puts it on the phone", p.env.clip === "for safari", p.env.clip);
  check("and the chip goes", p.chip() === null, p.chip());

  p.leave(); await p.sleep(1600);
  p.env.clip = "typed on the phone";
  await p.come();
  check("Safari never reads by itself", p.env.reads === 0 && p.pushes().length === 0, [p.env.reads, p.pushes()]);
  check("it offers Send instead", p.chip()?.kind === "send", p.chip());

  // A history entry's Send button sends this phone's clipboard straight to that computer.
  p.tap({ "data-clip": "paste", "data-key": "laptop" });
  await p.sleep(40);
  check("Send this phone's clipboard: one tap", p.pushes().at(-1)?.[1].data === "typed on the phone", p.pushes());
}

// ── the native app ────────────────────────────────────────────────────────────────────────
{
  const p = page("native");
  p.links([LAPTOP]);
  p.leave();
  p.W.clipShare.now("laptop", now("to android"));
  await p.sleep();
  check("native: written even in the background", p.env.nativeSets[0]?.data === "to android", p.env.nativeSets);
  check("and never marked sensitive (secrets never come)", p.env.nativeSets[0]?.sensitive === false, p.env.nativeSets[0]);
  await p.come();
  check("native read-back is an echo", p.pushes().length === 0, p.pushes());

  // A password manager's copy on the phone.
  p.env.clip = "s3cret!"; p.env.sensitive = true;
  p.W.clipNativeChanged();
  await p.sleep(40);
  check("a sensitive copy is never sent by itself", p.pushes().length === 0, p.pushes());
  p.tap({ "data-clip": "paste", "data-key": "laptop" });
  await p.sleep(40);
  const last = p.pushes().at(-1);
  check("sent by a deliberate tap, it goes marked sensitive", last?.[1].sensitive === true, last);

  p.env.clip = "ordinary"; p.env.sensitive = false;
  p.W.clipNativeChanged();
  await p.sleep(40);
  check("the app's change notice sends a new copy", p.pushes().at(-1)?.[1].data === "ordinary", p.pushes());
}

// ── the Android app's message bridge (F9) ──────────────────────────────────────────────────
{
  const p = page("app");
  p.links([LAPTOP]);
  p.leave();
  p.W.clipShare.now("laptop", now("to the app"));
  await p.sleep();
  check("app: the copy goes to the app to write, in the background too",
    JSON.stringify(p.env.appSent[0]) === JSON.stringify(["clip", { text: "to the app", sensitive: false }]), p.env.appSent);
  p.env.clip = "page must not read";
  await p.come();
  check("app: the page never reads the clipboard itself", p.env.reads === 0 && p.pushes().length === 0, [p.env.reads, p.pushes()]);
  p.W.clipShare.fromApp("to the app");
  await p.sleep(40);
  check("app: its read-back of our own write is an echo", p.pushes().length === 0, p.pushes());
  p.W.clipShare.fromApp("copied in another app");
  await p.sleep(40);
  check("app: the phone's copy it hands over goes to the computer", p.pushes().at(-1)?.[1].data === "copied in another app", p.pushes());
  p.W.clipSet("laptop", "push", false);
  p.W.clipShare.fromApp("switched off");
  await p.sleep(40);
  check("app: not with the switch off", p.pushes().length === 1, p.pushes());
}

console.log(failures ? `\n${failures} failed` : "\nall passed");
process.exit(failures ? 1 : 0);
