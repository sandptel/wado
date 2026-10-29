// Runnable check for the Android app's side of the client: js/native.js, native_sync.js,
// native_feel.js, native_share.js, native_push.js (docs/android.md).
//
// Two worlds: a browser (no `window.wadoNative`) — where nothing may change at all — and the
// app's WebView, with a fake `wadoNative` recording what the page posts and able to answer.
//   node scripts/native-check.mjs
import { readFileSync } from "node:fs";

const FILES = ["native.js", "native_sync.js", "native_feel.js", "native_share.js", "native_push.js"];
const src = FILES.map((f) => readFileSync(new URL("../crates/client/src/js/" + f, import.meta.url), "utf8")).join("\n");

let failures = 0;
const check = (name, ok, got) => { if (!ok) failures++; console.log(`${ok ? "ok  " : "FAIL"} ${name}${ok ? "" : " — got " + JSON.stringify(got)}`); };

function world(native) {
  const emitted = [];
  const style = new Map();
  const hexes = ["181818", "282828", "383838", "585858", "b8b8b8", "d8d8d8", "e8e8e8", "f8f8f8",
    "ab4642", "dc9656", "f7ca88", "a1b56c", "86c1b9", "7cafc2", "ba8baf", "a16946"];
  hexes.forEach((h, i) => style.set("--base0" + i.toString(16).toUpperCase(), "#" + h));
  const document = { documentElement: { style: { getPropertyValue: (k) => style.get(k) || "" } } };
  const settings = { profile: 0, profiles: [{ name: "Den", host: "den", relay_url: "https://r", remote_id: "42", conn_mode: "relay", secret: "x" }] };
  const sent = [];
  const handlers = {};
  const W = {
    _relayHandlers: handlers,
    relayOn: (t, fn) => { handlers[t] = fn; },
    relayUp: true,
    relaySendMsg: (m) => { sent.push(m); return true; },
    applyTheme: (h) => Array.isArray(h) && h.length === 16,
    saveSettings: () => {},
    loadSettings: () => settings,
    hostDo: (a) => sent.push({ hostDo: a }),
    clipboardSend: (t) => sent.push({ clip: t }),
    wake: { off: false, acquire: async () => "acq", release: () => "rel" },
    setWake: () => {},
    padConfirm: { show: () => "shown" },
    padTouch: { input: () => {} },
    hostState: null,
  };
  const vib = () => "browser";
  const navigator = { vibrate: vib };
  const window = native ? { wadoNative: native, wadoNativeInfo: { platform: "android", version: "0.0.3" } } : {};
  const before = { applyTheme: W.applyTheme, saveSettings: W.saveSettings, acquire: W.wake.acquire, show: W.padConfirm.show, input: W.padTouch.input };
  new Function("W", "emit", "window", "navigator", "document", "performance", "fetch", "File", src)(
    W, (m) => emitted.push(m), window, navigator, document, { now: () => Date.now() },
    async () => ({ ok: true, blob: async () => "blob" }), class { constructor(b, n) { this.name = n; } },
  );
  return { W, emitted, sent, handlers, navigator, vib, before, settings };
}

// ── a browser ───────────────────────────────────────────────────────────────────────────
{
  const w = world(null);
  check("browser: W.native is null", w.W.native === null, w.W.native);
  check("browser: nothing is wrapped",
    w.W.applyTheme === w.before.applyTheme && w.W.saveSettings === w.before.saveSettings &&
    w.W.wake.acquire === w.before.acquire && w.W.padConfirm.show === w.before.show &&
    w.W.padTouch.input === w.before.input, null);
  check("browser: navigator.vibrate untouched", w.navigator.vibrate === w.vib, null);
  check("browser: no relay handler added", Object.keys(w.handlers).length === 0, Object.keys(w.handlers));
}

// ── the app ─────────────────────────────────────────────────────────────────────────────
{
  const posted = [];
  const N = { postMessage: (s) => posted.push(JSON.parse(s)), onmessage: null };
  const w = world(N);
  const of = (t) => posted.filter((m) => m.t === t);
  const answer = (m) => N.onmessage({ data: JSON.stringify(m) });

  check("app: W.native is set with its info", w.W.native && w.W.native.info.version === "0.0.3", w.W.native && w.W.native.info);
  check("app: the scheme in use is reported at load", of("theme")[0]?.hexes?.[13] === "7cafc2", of("theme"));
  const c = of("computers")[0];
  check("app: computers are reported, keyed like the client", c && c.profiles[0].key === "https://r|42" && c.profiles[0].name === "Den", c);
  check("app: …and never carry anything else", c && !("secret" in c.profiles[0]), c);

  w.W.saveSettings({ profile: 0, profiles: [] });
  check("app: a save reports the computers again", of("computers").length === 2 && of("computers")[1].profiles.length === 0, of("computers"));
  w.W.applyTheme(Array(16).fill("ABCDEF"));
  check("app: a scheme change is reported, lower case", of("theme").at(-1).hexes[0] === "abcdef", of("theme").at(-1));

  w.navigator.vibrate(4); w.navigator.vibrate(12); w.navigator.vibrate(40);
  check("app: vibrate(ms) becomes predefined effects", JSON.stringify(of("haptic").map((m) => m.kind)) === '["tick","tap","heavy"]', of("haptic"));
  w.W.padConfirm.show(null, {}, { ok: false });
  check("app: a refused pad action rejects in the hand", of("haptic").at(-1).kind === "reject", of("haptic").at(-1));
  w.W.padTouch.input("down", { id: 1, x: 10, y: 10 });
  w.W.padTouch.input("up", { id: 1, x: 12, y: 11 });
  check("app: a quick one-finger tap ticks", of("haptic").at(-1).kind === "tap", of("haptic").at(-1));
  const n0 = of("haptic").length;
  w.W.padTouch.input("down", { id: 2, x: 10, y: 10 });
  w.W.padTouch.input("up", { id: 2, x: 90, y: 10 });
  check("app: a drag does not", of("haptic").length === n0, of("haptic").length);

  await w.W.wake.acquire();
  check("app: a wake lock asks for the screen on + immersive", of("active").at(-1)?.on === true, of("active"));
  w.W.wake.release();
  check("app: …and lets go", of("active").at(-1)?.on === false, of("active"));

  w.handlers.clipboard({ text: "copied", sensitive: true });
  check("app: the computer's copy goes to the phone, sensitivity kept", of("clip").at(-1)?.text === "copied" && of("clip").at(-1).sensitive === true, of("clip"));
  answer({ t: "clip", text: "from phone" });
  check("app: the phone's copy goes to the computer", w.sent.some((m) => m.clip === "from phone"), w.sent);

  w.W.hostState = {
    media: [{ bus: "org.mpris.x", app: "Spotify", title: "Song", artist: "A", playing: true, position_ms: 5, length_ms: 9, can_next: true }],
    audio: { default_sink: "s", sinks: [{ id: 7, name: "s", label: "Speakers", volume: 0.5, muted: false }] },
  };
  w.handlers.host_state({ state: w.W.hostState });
  const np = of("nowPlaying").at(-1);
  check("app: now playing is mirrored", np && np.title === "Song" && np.volume === 0.5 && np.can_next === true, np);
  answer({ t: "media", op: "next" });
  check("app: a media key drives the computer's player",
    JSON.stringify(w.sent.at(-1)) === JSON.stringify({ hostDo: { do: "media", bus: "org.mpris.x", op: { op: "next" } } }), w.sent.at(-1));
  answer({ t: "volume", volume: 0.8 });
  check("app: a volume key sets the default output", w.sent.at(-1).hostDo?.do === "sink_volume" && w.sent.at(-1).hostDo.id === 7 && w.sent.at(-1).hostDo.volume === 0.8, w.sent.at(-1));

  w.handlers.notification({ id: 3, app: "Mail", summary: "New", body: "hi" });
  const nt = of("notify").at(-1);
  check("app: a session app's notification becomes a system one", nt && nt.kind === "app" && nt.id === "3" && nt.title === "Mail — New", nt);
  let answered = null;
  w.W.nativeActions.claude_permission = (id, a) => { answered = [id, a]; };
  answer({ t: "notifyAction", kind: "claude_permission", id: "9", action: "allow" });
  check("app: a notification button reaches its feature", JSON.stringify(answered) === '["9","allow"]', answered);

  const sub = w.W.native.pushSubscribe("BKey");
  check("app: push subscribe asks the app with the VAPID key", of("pushSubscribe").at(-1)?.vapid === "BKey", of("pushSubscribe"));
  answer({ t: "pushEndpoint", endpoint: "https://ntfy/x", keys: { p256dh: "p", auth: "a" } });
  const got = await sub;
  check("app: …and resolves to a Web Push subscription", got.endpoint === "https://ntfy/x" && got.keys.auth === "a", got);

  answer({ t: "back" });
  check("app: Back is handed to Rust", w.emitted.some((m) => m.type === "nativeBack"), w.emitted);
  answer({ t: "go", key: "https://r|42", go: "trackpad" });
  check("app: go opens a computer's door through Rust", w.emitted.some((m) => m.type === "nativeGo" && m.go === "trackpad"), w.emitted);

  answer({ t: "share", key: "https://r|42", files: [], links: ["https://example.com/x"], text: "see https://example.com/x" });
  await new Promise((r) => setTimeout(r, 10));
  check("app: a shared link opens on the computer", w.sent.some((m) => m.type === "session_launch" && m.command === "xdg-open 'https://example.com/x'"), w.sent);
  check("app: …and the share is reported done", of("shareDone").at(-1)?.ok === true, of("shareDone"));
  answer({ t: "share", key: "https://r|42", files: [], links: ["javascript:alert(1)"], text: "" });
  await new Promise((r) => setTimeout(r, 10));
  check("app: only http(s) links are opened", !w.sent.some((m) => m.command && m.command.includes("javascript")), w.sent);
}

console.log(failures ? `\n${failures} failed` : "\nall passed");
process.exit(failures ? 1 : 0);
