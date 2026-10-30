// wado bridge — the clipboard shared both ways, by itself, with every computer this phone holds a
// link to (the selected one's relay link, and each widget's light link: side_link.js).
//
// Computer → phone ("Copy from this computer automatically"): the link asks for every new copy
// (`clip_watch {live}`); each `clip_now` is written to this phone's clipboard as soon as the
// browser allows. Chrome and Android write while the page has focus; a copy that arrives in the
// background waits and is written on the next focus. Where a write is refused (Safari, which
// writes only inside a tap), the copy waits in a one-tap chip, already prepared, so the tap is the
// write. The native app (window.wadoNative, F9) writes whenever.
//
// Phone → computer ("Send this phone's copies automatically"): when the page comes back (focus,
// visible), this phone's clipboard is read — silently where reading is already allowed (Chrome
// with the permission granted; the native app), else a one-tap "Send" chip offers it — and sent
// if it is new. A copy the phone marked sensitive (a password manager's) is never sent by itself.
//
// Echo: every value is named by the same hash the daemon uses (the first 16 bytes of SHA-256, hex).
// `last` is the value this phone's clipboard is known to hold — written here, or read and sent —
// so a copy coming back the way it went stops at once, on either side.
//
// The native hook, feature-detected (F9's Android app implements it; the web path is the default):
//   window.wadoNative.clipboard = {
//     set({ mime, data, sensitive }) → bool | Promise<bool>   // data: the text, or a data: URI
//     get() → { mime, data, sensitive } | null | Promise<…>
//   }
//   and the app calls window.__wado.clipNativeChanged() when the phone's clipboard changes.
// The Android app (F9, native.js) speaks its own message bridge instead: a copy for the phone goes
// to it as `clip {text, sensitive}` (it writes it, focused or not), and the phone's copy comes back
// from it (native_sync.js → W.clipShare.fromApp) — the app reads only when it may, and never a
// copy marked sensitive. Pictures still take the web path.
//
// From Rust:  W.clipLinks([{ key, name, selected, may, paired }])   the computers, and what each may
//             W.clipSet(key, which, on)   which: "pull" | "push" — this phone's switch for that computer
// To Rust:    {type:"clipShare", prefs:{key:{pull,push}}} · {type:"clipChip", chip|null}
//             · {type:"clipSaid", text, tone}
// Taps: any element with data-clip="chip-copy|chip-send|chip-close|copy|send|paste" (data-key,
// data-id, data-kind) is handled here, inside the tap, where a browser lets the clipboard be used.

(() => {
  const MAX_TEXT = 1 << 20, MAX_IMAGE = 8 << 20;
  const PREFS = "wado.clipShare", LAST = "wado.clipLast";
  const S = {
    links: new Map(),   // key -> { key, name, selected, may, paired }
    watched: new Map(), // key -> the `live` this link was last told
    prefs: {},          // key -> { pull?, push? } as the person set them
    last: "",           // hash of what this phone's clipboard holds, as far as wado knows
    skipImage: false,   // an image was just written: its read-back is re-encoded, not new
    pending: null,      // a computer's copy not yet on this phone
    chip: null,
    acks: new Map(),    // hash -> keys a push awaits
    hiddenAt: 0,
    reading: false,
    readAt: 0,
    heldSaid: new Set(), // "key id-or-hash": a too-big copy already said once
  };
  const store = (() => { try { return window.localStorage; } catch (_) { return null; } })();
  try { S.prefs = JSON.parse(store.getItem(PREFS)) || {}; } catch (_) {}
  try { S.last = store.getItem(LAST) || ""; } catch (_) {}

  // The app's message bridge (native.js), as the same two calls: it writes text whenever, and it
  // reads on its own — so `get` is left out and the page never reads the clipboard itself.
  const appBridge = () => (W.native && typeof W.native.send === "function" ? {
    set: (o) => { if (String(o.mime).startsWith("image/")) return false; W.native.send("clip", { text: o.data, sensitive: !!o.sensitive }); return true; },
    app: true,
  } : null);
  const native = () => {
    try { const n = window.wadoNative; if (n && n.clipboard) return n.clipboard; } catch (_) {}
    return appBridge();
  };
  const nameOf = (key) => (S.links.get(key) || {}).name || "the computer";
  const deflt = (key) => { const l = S.links.get(key); return !!(l && l.may && l.paired); };
  const pref = (key, which) => {
    const p = S.prefs[key] || {};
    return typeof p[which] === "boolean" ? p[which] : deflt(key);
  };
  const on = (key, which) => { const l = S.links.get(key); return !!(l && l.may) && pref(key, which); };
  const remember = (h) => { S.last = h; try { store.setItem(LAST, h); } catch (_) {} };

  // The daemon's words name "the computer"; this phone knows its name.
  const onIt = (key, why) => String(why || "").replace("the computer", nameOf(key));
  function say(text, tone) { emit({ type: "clipSaid", text, tone: tone || "good" }); }
  function setChip(c) { S.chip = c; emit({ type: "clipChip", chip: c }); }
  function share() {
    const prefs = {};
    for (const key of S.links.keys()) prefs[key] = { pull: pref(key, "pull"), push: pref(key, "push") };
    const j = JSON.stringify(prefs);
    if (j === S.shared) return;
    S.shared = j;
    emit({ type: "clipShare", prefs });
  }

  // The link to `key`, if it is up: the selected computer's relay link, or its light link.
  function up(key) {
    const l = S.links.get(key);
    if (!l) return false;
    return l.selected ? !!W.relayUp : !!(W.sideUp && W.sideUp(key));
  }
  function send(key, msg) {
    const l = S.links.get(key);
    if (!l || !up(key)) return false;
    if (l.selected) { W.relaySendMsg(msg); return true; }
    return !!(W.sideDo && W.sideDo(key, msg));
  }

  // Tell each link that is up what it should send — once per link, again on a change.
  function sync() {
    for (const [key, l] of S.links) {
      if (!l.may || !up(key)) continue;
      const live = pref(key, "pull");
      if (S.watched.get(key) === live) continue;
      if (send(key, { type: "clip_watch", live })) S.watched.set(key, live);
    }
  }

  // ── hashing ────────────────────────────────────────────────────────────────────────────────
  async function hash(bytes) {
    try {
      const d = new Uint8Array(await crypto.subtle.digest("SHA-256", bytes));
      return Array.from(d.slice(0, 16), (b) => b.toString(16).padStart(2, "0")).join("");
    } catch (_) {
      // No WebCrypto (an insecure origin): a local name, which only dedupes on this phone.
      let h = 0x811c9dc5;
      for (const b of bytes) h = Math.imul(h ^ b, 16777619) >>> 0;
      return "f" + h.toString(16) + ":" + bytes.length;
    }
  }
  const utf8 = (t) => new TextEncoder().encode(t);
  const dataUrlBytes = (u) => {
    const b64 = String(u).split(",")[1] || "";
    const bin = atob(b64);
    const out = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
    return out;
  };
  const toDataUrl = (blob) => new Promise((res, rej) => {
    const r = new FileReader();
    r.onload = () => res(r.result);
    r.onerror = () => rej(r.error);
    r.readAsDataURL(blob);
  });
  // A browser's clipboard holds PNG only.
  async function pngOf(dataUrl) {
    const blob = await (await fetch(dataUrl)).blob();
    if (blob.type === "image/png") return blob;
    const bmp = await createImageBitmap(blob);
    const c = document.createElement("canvas");
    c.width = bmp.width; c.height = bmp.height;
    c.getContext("2d").drawImage(bmp, 0, 0);
    return new Promise((res) => c.toBlob(res, "image/png"));
  }

  // ── computer → phone ───────────────────────────────────────────────────────────────────────
  // Start the write now — inside a tap when there is one — and say whether it took.
  function write(o) {
    const image = String(o.mime).startsWith("image/");
    const n = native();
    let p;
    try {
      if (n && n.set && !(n.app && image)) {
        p = Promise.resolve(n.set({ mime: image ? o.mime : "text/plain", data: o.data, sensitive: false }))
          .then((ok) => { if (ok === false) throw new Error("refused"); });
      } else if (!navigator.clipboard) {
        p = Promise.reject(new Error("no clipboard"));
      } else if (image) {
        if (typeof ClipboardItem === "undefined") throw new Error("no images");
        p = navigator.clipboard.write([new ClipboardItem({ "image/png": pngOf(o.data) })]);
      } else {
        p = navigator.clipboard.writeText(o.data);
      }
    } catch (e) {
      p = Promise.reject(e);
    }
    return p.then(() => {
      remember(o.hash);
      S.skipImage = image;
      if (S.pending && S.pending.hash === o.hash) S.pending = null;
      if (S.chip && S.chip.kind === "copy") setChip(null);
      say(image ? `Picture from ${nameOf(o.key)} copied to this phone` : `Copied to this phone from ${nameOf(o.key)}`);
      return true;
    }, () => false);
  }

  const focused = () => document.visibilityState !== "hidden" && (!document.hasFocus || document.hasFocus());
  const preview = (o) => String(o.mime).startsWith("image/") ? "" : String(o.data).replace(/\s+/g, " ").trim().slice(0, 80);

  function offerChip(o) {
    setChip({ kind: "copy", key: o.key, name: nameOf(o.key), preview: preview(o),
      image: String(o.mime).startsWith("image/"), from: o.from || "", id: o.hash });
  }

  async function deliver(o) {
    if ((focused() || native()) && await write(o)) return;
    // Refused, or not ours to write yet. A copy that arrived in the background is tried again
    // on focus; one refused while focused needs a tap.
    if (focused() || native()) offerChip(o);
  }

  function now(key, m) {
    if (!on(key, "pull") || !m || !m.hash || m.hash === S.last) return;
    const o = { key, hash: m.hash, mime: m.mime || "text/plain", data: m.data || "", from: m.from || "", at: m.at_ms || 0 };
    S.pending = o;
    deliver(o);
  }

  // ── phone → computer ───────────────────────────────────────────────────────────────────────
  // What this phone's clipboard holds: { mime, data, bytes, sensitive } or null.
  async function read() {
    const n = native();
    if (S.fromApp) {
      const t = S.fromApp;
      S.fromApp = null;
      return { mime: "text/plain", data: t, bytes: utf8(t), sensitive: false };
    }
    if (n && n.get) {
      const g = await n.get();
      if (!g || !g.data) return null;
      const image = String(g.mime || "").startsWith("image/");
      return { mime: image ? g.mime : "text/plain", data: g.data, bytes: image ? dataUrlBytes(g.data) : utf8(g.data), sensitive: !!g.sensitive };
    }
    if (!navigator.clipboard) return null;
    if (navigator.clipboard.read) {
      for (const item of await navigator.clipboard.read()) {
        const img = item.types.find((t) => t === "image/png" || t === "image/jpeg");
        if (img) {
          const blob = await item.getType(img);
          if (blob.size > MAX_IMAGE) return { mime: img, data: "", bytes: new Uint8Array(0), tooBig: true };
          return { mime: img, data: await toDataUrl(blob), bytes: new Uint8Array(await blob.arrayBuffer()), sensitive: false };
        }
        if (item.types.includes("text/plain")) {
          const t = await (await item.getType("text/plain")).text();
          return t ? { mime: "text/plain", data: t, bytes: utf8(t), sensitive: false } : null;
        }
      }
      return null;
    }
    const t = await navigator.clipboard.readText();
    return t ? { mime: "text/plain", data: t, bytes: utf8(t), sensitive: false } : null;
  }

  // May the clipboard be read now without a tap? "yes" | "tap" | "no".
  async function readable() {
    const n = native();
    if (n && n.app) return "no"; // the app sends the phone's copies itself (fromApp)
    if (n) return "yes";
    if (!navigator.clipboard) return "no";
    try {
      const st = await navigator.permissions.query({ name: "clipboard-read" });
      return st.state === "granted" ? "yes" : st.state === "denied" ? "no" : "tap";
    } catch (_) {
      return "tap"; // Safari and Firefox: no such permission, a tap is the permission
    }
  }

  // Read and send to `keys`. `manual`: a tap asked, so say what happened either way.
  async function sendFrom(keys, manual) {
    if (!keys.length) return;
    let got;
    try { got = await read(); } catch (_) {
      if (manual) say("This browser would not share its clipboard", "bad");
      return;
    }
    if (!got) { if (manual) say("This phone's clipboard is empty", "dim"); return; }
    if (got.sensitive && !manual) return; // a password manager's copy: never by itself
    const tooBig = got.tooBig || (got.mime === "text/plain" ? got.bytes.length > MAX_TEXT : got.bytes.length > MAX_IMAGE);
    if (tooBig) { if (manual) say(got.mime === "text/plain" ? "That text is too long to send" : "That picture is too big to send", "bad"); return; }
    const h = await hash(got.bytes);
    if (!manual) {
      if (h === S.last) return;
      // The picture just written here, read back re-encoded: the same copy, not a new one.
      if (S.skipImage && got.mime !== "text/plain") { S.skipImage = false; remember(h); return; }
    }
    const msg = { type: "clip_push", mime: got.mime, data: got.data, sensitive: !!got.sensitive };
    const sent = keys.filter((k) => send(k, msg));
    if (!sent.length) { if (manual) say(`${keys.length === 1 ? nameOf(keys[0]) : "The computer"} is not connected`, "bad"); return; }
    S.acks.set(h, sent);
    remember(h);
    S.skipImage = false;
  }

  function pushed(key, m) {
    const keys = S.acks.get(m && m.hash);
    if (!keys || !keys.includes(key)) return;
    S.acks.set(m.hash, keys.filter((k) => k !== key));
    if (!S.acks.get(m.hash).length) S.acks.delete(m.hash);
    say(`Sent to ${nameOf(key)}`);
  }

  // The page came back: put a waiting copy on this phone, then offer this phone's.
  async function back() {
    if (!focused()) return;
    // Taken before the write: focus and visibility arrive together, and the copy goes once.
    const o = S.pending;
    S.pending = null;
    if (o && !(await write(o))) {
      if (!S.pending) S.pending = o;
      offerChip(o);
    }
    // Focus and visibility arrive together: one read for both.
    const t = Date.now();
    if (S.reading || t - S.readAt < 800) return;
    S.readAt = t; S.reading = true;
    try {
      const keys = [...S.links.keys()].filter((k) => on(k, "push") && up(k));
      if (!keys.length) return;
      const how = await readable();
      if (how === "yes") await sendFrom(keys, false);
      else if (how === "tap" && !S.pending && S.hiddenAt && t - S.hiddenAt > 1500 && !(S.chip && S.chip.kind === "copy")) {
        // Offered, not pressed on the person: it goes again if they ignore it.
        setChip({ kind: "send", key: keys[0], keys, name: keys.length === 1 ? nameOf(keys[0]) : "your computers", id: "send" + t });
        setTimeout(() => { if (S.chip && S.chip.id === "send" + t) setChip(null); }, 9000);
      }
    } finally {
      S.reading = false;
    }
  }

  // ── taps ───────────────────────────────────────────────────────────────────────────────────
  function mark(el, state) {
    el.setAttribute("data-done", state);
    setTimeout(() => { if (el.getAttribute("data-done") === state) el.removeAttribute("data-done"); }, 1600);
  }

  function tap(e) {
    const el = e.target && e.target.closest && e.target.closest("[data-clip]");
    if (!el) return;
    const act = el.getAttribute("data-clip"), key = el.getAttribute("data-key") || "";
    switch (act) {
      case "chip-copy":
        if (S.pending) write(S.pending).then((ok) => { if (!ok) say("This browser would not take it — try again", "bad"); });
        break;
      case "chip-send": {
        const keys = (S.chip && S.chip.keys) || [key];
        setChip(null);
        sendFrom(keys, true);
        break;
      }
      case "chip-close":
        if (S.chip && S.chip.kind === "copy") S.pending = null;
        setChip(null);
        break;
      case "paste":
        sendFrom([key], true);
        break;
      case "copy": {
        const l = S.links.get(key) || {};
        const id = el.getAttribute("data-id"), kind = el.getAttribute("data-kind");
        const done = (ok, why) => {
          mark(el, ok ? "ok" : "no");
          if (ok) say(`Copied to this phone from ${nameOf(key)}`);
          else if (why) say(onIt(key, why), "dim");
          else say("This browser would not take it — tap again", "bad");
        };
        if (l.selected && W.clipCopy) W.clipCopy(id, kind, done);
        else if (W.sideClipCopy) W.sideClipCopy(key, id, kind, done);
        break;
      }
      case "send": {
        // An entry of the history back onto the computer's clipboard.
        const l = S.links.get(key) || {};
        const id = el.getAttribute("data-id");
        const fetchIt = l.selected ? W.clipFetchEntry : W.sideClipFetch;
        if (!fetchIt) break;
        Promise.resolve(fetchIt(l.selected ? id : key, l.selected ? undefined : id)).then((c) => {
          const msg = { type: "clip_push", mime: c.mime, data: c.data };
          const bytes = String(c.mime).startsWith("image/") ? dataUrlBytes(c.data) : utf8(c.data);
          return hash(bytes).then((h) => {
            if (!send(key, msg)) throw new Error("down");
            S.acks.set(h, [key]);
            mark(el, "ok");
          });
        }).catch((e) => {
          mark(el, "no");
          if (e && e.held) say(onIt(key, e.message), "dim");
          else say(`${nameOf(key)} did not answer`, "bad");
        });
        break;
      }
      default: return;
    }
  }

  // ── wiring ─────────────────────────────────────────────────────────────────────────────────
  W.clipShare = {
    now, pushed, back, sync,
    // Too big for the relay's lane (`clip_held`): said once per entry or copy.
    held(key, m) {
      const k = `${key} ${(m && (m.id || m.hash)) || ""}`;
      if (S.heldSaid.has(k)) return;
      S.heldSaid.add(k);
      say(onIt(key, m && m.message), "dim");
    },
    error(key, m) { if (S.acks.size) say((m && m.message) || `${nameOf(key)} did not take it`, "bad"); },
    // A link came (back) up: it has forgotten what it was told.
    linkUp(key) { S.watched.delete(key); sync(); },
    mainKey() { for (const [k, l] of S.links) if (l.selected) return k; return ""; },
    _state: S,
  };
  W.clipLinks = (list) => {
    const next = new Map((list || []).map((l) => [l.key, l]));
    for (const [k, l] of next) {
      const was = S.links.get(k);
      // The link behind a key changed (selected ↔ light): it has to be told again.
      if (!was || was.selected !== l.selected || was.may !== l.may) S.watched.delete(k);
    }
    S.links = next;
    share();
    sync();
  };
  W.clipSet = (key, which, value) => {
    S.prefs[key] = Object.assign({}, S.prefs[key], { [which]: !!value });
    try { store.setItem(PREFS, JSON.stringify(S.prefs)); } catch (_) {}
    if (which === "pull") { S.watched.delete(key); if (!value && S.pending && S.pending.key === key) { S.pending = null; if (S.chip && S.chip.kind === "copy") setChip(null); } }
    share();
    sync();
  };
  W.clipNativeChanged = () => { S.readAt = 0; back(); };
  // The app read the phone's copy (never a sensitive one): to every computer whose switch is on.
  W.clipShare.fromApp = (text) => {
    const keys = [...S.links.keys()].filter((k) => on(k, "push") && up(k));
    if (!text || !keys.length) return;
    S.fromApp = String(text);
    sendFrom(keys, false);
  };

  document.addEventListener("click", tap, true);
  window.addEventListener("focus", () => back());
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "hidden") S.hiddenAt = Date.now();
    else back();
  });
})();
