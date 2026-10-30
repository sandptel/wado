// wado bridge — "Send to computer" from the Android share sheet (native.js; nothing without it).
//
//   app → page  share  {key, files: [{url, name, mime, size}], links: [url], text}
//   page → app  shareDone {ok, why}
//
// The app has already asked which computer; `key` is its profile key. Files are not copied
// into the page: each `url` is served by the app itself, on this page's own origin
// (/share/<token>/<n>), straight from the other app's content, and uploaded from there into
// the computer's ~/Downloads through the file link (js/files_xfer.js), where the transfer list
// shows its progress. A link opens on the computer, in its running session.

(() => {
  const N = W.native;
  if (!N) return;

  const currentKey = () => {
    const s = W.loadSettings();
    const p = (Array.isArray(s.profiles) ? s.profiles : [])[s.profile | 0];
    return p ? (p.relay_url || "") + "|" + (p.remote_id || "") : "";
  };
  const until = (ok, ms) => new Promise((done, fail) => {
    const t0 = Date.now();
    const tick = () => {
      if (ok()) return done();
      if (Date.now() - t0 > ms) return fail(new Error("the computer did not answer"));
      setTimeout(tick, 200);
    };
    tick();
  });
  const quote = (p) => "'" + String(p).replace(/'/g, "'\\''") + "'";

  N.on("share", async (m) => {
    const files = Array.isArray(m.files) ? m.files : [];
    const links = Array.isArray(m.links) ? m.links : [];
    try {
      if (m.key && m.key !== currentKey()) emit({ type: "nativeGo", key: m.key, go: files.length ? "files" : "" });
      await until(() => W.relayUp, 25000);
      if (files.length) {
        if (!W.files || !W.files.connect) throw new Error("this client has no file link");
        W.filesOpen && W.filesOpen();
        const info = await W.files.connect();
        const got = [];
        for (const f of files) {
          const r = await fetch(f.url);
          if (!r.ok) throw new Error("could not read " + (f.name || "a file"));
          got.push(new File([await r.blob()], f.name || "shared", { type: f.mime || "" }));
        }
        W.files.upload(String(info.home || "~").replace(/\/$/, "") + "/Downloads", got);
      }
      for (const url of links) {
        if (!/^https?:\/\//i.test(url)) continue;
        if (!W.relaySendMsg({ type: "session_launch", command: "xdg-open " + quote(url) })) throw new Error("not connected");
      }
      if (m.text && !links.length && W.clipboardSend) W.clipboardSend(String(m.text));
      N.send("shareDone", { ok: true });
    } catch (e) {
      N.send("shareDone", { ok: false, why: String((e && e.message) || e) });
    }
  });
})();
