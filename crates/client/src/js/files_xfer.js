// wado bridge — the file manager's transfers: a queue, two at a time, each pausable and
// resumable, with speed and time left (Decision Log 2026-10-04, item 9).
//
// Downloads land in OPFS — on disk, surviving a reload — and resume from what is there. The
// finished file goes to the browser's own Downloads from that disk-backed File, so nothing is
// held in memory and only the disk limits the size. Every download is checked against the
// computer's SHA-256 of the whole file; an upload is checked the same way by the computer before
// it is renamed into place.
//
//   W.files.download(path, name, isDir)   W.files.upload(dir, fileList)
//   W.files.xfers                         the list, newest last
//   W.files.pause(x) / resume(x) / cancel(x) / clearDone()
//   W.files.onXfer(fn)                    fn() after any change

(() => {
  const F = W.files;
  const MAX_RUNNING = 2;
  const HIGH = 1 << 20; // upload: wait while this much is queued in the channel
  const CHUNK = 60 * 1024; // = wado_protocol::files::CHUNK
  const SAVED = "wado.xfers";
  F.xfers = [];
  const listeners = [];
  F.onXfer = (fn) => listeners.push(fn);
  let changed = false;
  function touch() {
    if (changed) return;
    changed = true;
    requestAnimationFrame(() => { changed = false; for (const fn of listeners) { try { fn(); } catch (_) {} } });
  }

  // ── persistence: downloads only (an upload's File does not survive a reload) ──
  function save() {
    const keep = F.xfers.filter((x) => x.kind !== "up" && x.state !== "error")
      .map(({ kind, path, name, size, opfs, state }) => ({ kind, path, name, size, opfs, state: state === "done" ? "done" : "paused" }));
    try { localStorage.setItem(SAVED, JSON.stringify(keep)); } catch (_) {}
  }
  async function opfsDir() {
    const root = await navigator.storage.getDirectory();
    return root.getDirectoryHandle("wado-downloads", { create: true });
  }
  (async () => {
    let saved = [];
    try { saved = JSON.parse(localStorage.getItem(SAVED)) || []; } catch (_) {}
    for (const s of saved) F.xfers.push({ ...s, done: 0, speed: 0, err: "" });
    // Bytes already on disk, for the bars; and files nobody is waiting for any more, gone.
    try {
      const dir = await opfsDir();
      const wanted = new Set(F.xfers.map((x) => x.opfs));
      for await (const [name, h] of dir.entries()) {
        if (!wanted.has(name)) { await dir.removeEntry(name).catch(() => {}); continue; }
        const x = F.xfers.find((y) => y.opfs === name);
        x.done = (await h.getFile()).size;
      }
    } catch (_) {}
    touch();
  })();

  const running = () => F.xfers.filter((x) => x.state === "running" || x.state === "verifying").length;
  function pump() {
    while (running() < MAX_RUNNING) {
      const next = F.xfers.find((x) => x.state === "queued");
      if (!next) break;
      start(next);
    }
    save();
    touch();
  }

  function rate(x) {
    const now = performance.now();
    if (!x.t0) { x.t0 = now; x.d0 = x.done; return; }
    const dt = (now - x.t0) / 1000;
    if (dt < 0.5) return;
    const inst = (x.done - x.d0) / dt;
    x.speed = x.speed ? x.speed * 0.6 + inst * 0.4 : inst;
    x.t0 = now; x.d0 = x.done;
  }

  function fail(x, why) {
    x.state = why === "lost" ? "waiting" : "error";
    x.err = why === "lost" ? "connection lost — resumes when it is back" : why;
    x.speed = 0;
    if (x.state === "waiting") retrySoon();
    pump();
  }

  // ── downloads ──────────────────────────────────────────────────────────────
  async function startDown(x) {
    const dir = await opfsDir();
    const fh = await dir.getFileHandle(x.opfs, { create: true });
    let have = (await fh.getFile()).size;
    if (x.kind === "zip") have = 0; // a zip restarts
    const sha = W.sha256();
    if (have && x.kind === "down") {
      // The hash covers the whole file: re-read what is already here.
      x.state = "verifying"; touch();
      const r = (await fh.getFile()).stream().getReader();
      for (;;) { const { value, done } = await r.read(); if (done) break; sha.update(value); }
    }
    const w = await fh.createWritable({ keepExistingData: have > 0 });
    if (have) await w.seek(have); else await w.truncate(0);
    x.done = have;
    x.state = "running";
    let chain = Promise.resolve();
    let finished = false;
    const stop = async () => { try { await chain; await w.close(); } catch (_) {} };
    x._stop = stop;
    x.id = F.stream(x.kind === "zip" ? "zip" : "get", x.kind === "zip" ? { path: x.path } : { path: x.path, offset: have }, {
      bytes(u8) {
        if (finished) return;
        if (x.kind === "down") sha.update(u8);
        chain = chain.then(() => w.write(u8));
        x.done += u8.length;
        rate(x); touch();
      },
      msg(m) {
        if (m.err) { finished = true; stop().then(() => fail(x, m.lost ? "lost" : m.err)); return true; }
        if (m.done) {
          finished = true;
          x.state = "verifying"; touch();
          stop().then(async () => {
            const ok = x.kind === "zip" ? x.done === x.size : sha.hex() === m.sha256;
            if (!ok) {
              await dir.removeEntry(x.opfs).catch(() => {});
              x.done = 0;
              return fail(x, "arrived damaged (hash mismatch) — download it again");
            }
            x.state = "done"; x.speed = 0;
            deliver(x, fh);
            pump();
          });
          return true;
        }
        if (m.ok && m.size !== undefined) { x.size = m.size; touch(); }
        return false;
      },
    });
  }

  // Into the browser's Downloads, from the file on disk.
  async function deliver(x, fh) {
    const file = await (fh || await (await opfsDir()).getFileHandle(x.opfs)).getFile();
    const a = document.createElement("a");
    a.href = URL.createObjectURL(file);
    a.download = x.name;
    document.body.appendChild(a);
    a.click();
    a.remove();
    setTimeout(() => URL.revokeObjectURL(a.href), 60000);
  }
  F.saveAgain = (x) => deliver(x).catch((e) => { x.err = String(e); touch(); });

  // ── uploads ────────────────────────────────────────────────────────────────
  async function startUp(x) {
    x.state = "running";
    touch();
    const put = await new Promise((ok) => {
      x.id = F.stream("put", { dir: x.dir, name: x.name, size: x.size, clash: x.clash }, {
        msg(m) {
          if (!x.started) { x.started = true; ok(m); return false; }
          // Later answers on the put's id are failures while writing.
          if (m.err) { x.broken = m.lost ? "lost" : m.err; return true; }
          return false;
        },
      });
    });
    if (put.err) {
      x.started = false;
      F.forget(x.id);
      if (put.err === "exists") return clash(x);
      return fail(x, put.lost ? "lost" : put.err);
    }
    // Resume under the name the computer chose, whatever happens to the original name later.
    const folder = x.name.includes("/") ? x.name.slice(0, x.name.lastIndexOf("/") + 1) : "";
    x.name = folder + put.name;
    if (x.clash !== "replace") x.clash = "fail";
    const sha = W.sha256();
    let pos = put.offset || 0;
    if (pos) {
      x.state = "verifying"; touch();
      const r = x.file.slice(0, pos).stream().getReader();
      for (;;) { const { value, done } = await r.read(); if (done) break; sha.update(value); }
      x.state = "running";
    }
    x.done = pos;
    while (pos < x.size) {
      if (x.state !== "running") return; // paused or cancelled
      if (x.broken) return fail(x, x.broken);
      if (!F.open()) return fail(x, "lost");
      while (F.buffered() > HIGH) {
        await new Promise((r) => setTimeout(r, 10));
        if (!F.open()) return fail(x, "lost");
      }
      const u8 = new Uint8Array(await x.file.slice(pos, pos + CHUNK).arrayBuffer());
      sha.update(u8);
      F.bytes(x.id, u8);
      pos += u8.length;
      x.done = pos;
      rate(x); touch();
    }
    x.state = "verifying"; touch();
    try {
      await F.req("put_end", { xfer: x.id, sha256: sha.hex() });
      x.state = "done"; x.speed = 0;
      F.forget(x.id);
      if (F.uploaded) F.uploaded(x);
    } catch (e) {
      F.forget(x.id);
      return fail(x, String(e.message || e));
    }
    pump();
  }

  // The name is taken: ask, unless an earlier answer was "for all".
  let clashAll = null;
  function clash(x) {
    const go = (how) => { x.clash = how; x.state = how === "skip" ? "skipped" : "queued"; pump(); };
    if (clashAll) return go(clashAll);
    x.state = "asking"; touch();
    const base = x.name.split("/").pop();
    const all = F.xfers.filter((y) => y.kind === "up" && (y.state === "queued" || y.state === "asking")).length > 1;
    const opts = [
      { label: "Replace (the old one goes to the Trash)", run: () => go("replace") },
      { label: "Keep both", run: () => go("rename") },
      { label: "Skip", run: () => go("skip") },
    ];
    if (all) {
      opts.push({ label: "Replace all", run: () => { clashAll = "replace"; go("replace"); } });
      opts.push({ label: "Keep both for all", run: () => { clashAll = "rename"; go("rename"); } });
      opts.push({ label: "Skip all", run: () => { clashAll = "skip"; go("skip"); } });
    }
    W.sheet.ask("files-clash", `“${base}” already exists here`, opts);
  }

  function start(x) {
    x.err = "";
    x.t0 = 0;
    x.speed = 0;
    x.broken = null;
    x.started = false;
    x.state = "running";
    F.connect()
      .then(() => (x.kind === "up" ? startUp(x) : startDown(x)))
      .catch((e) => fail(x, F.ready ? String(e.message || e) : "lost"));
  }

  // ── after a drop: try again while anything is waiting ──────────────────────
  let retryTimer = null;
  function retrySoon() {
    if (retryTimer) return;
    retryTimer = setTimeout(() => {
      retryTimer = null;
      const waiting = F.xfers.filter((x) => x.state === "waiting");
      if (!waiting.length) return;
      if (!W.e2eReady) return retrySoon();
      F.connect().then(() => { for (const x of waiting) x.state = "queued"; pump(); }).catch(() => retrySoon());
    }, 3000);
  }
  F.onDown(() => { for (const x of F.xfers) if (x.state === "running") x.state = "waiting"; retrySoon(); touch(); });

  // ── the public verbs ───────────────────────────────────────────────────────
  F.download = (path, name, isDir) => {
    const x = {
      kind: isDir ? "zip" : "down", path, name: isDir ? name + ".zip" : name, size: 0, done: 0,
      opfs: Date.now().toString(36) + Math.random().toString(36).slice(2, 8), state: "queued", speed: 0, err: "",
    };
    F.xfers.push(x);
    pump();
  };
  F.upload = (dir, files) => {
    clashAll = null;
    for (const f of files) {
      F.xfers.push({
        kind: "up", file: f, dir, name: f.wadoPath || f.webkitRelativePath || f.name, size: f.size,
        done: 0, state: "queued", speed: 0, err: "", clash: "fail",
      });
    }
    pump();
  };
  F.pause = async (x) => {
    if (x.state !== "running" && x.state !== "queued" && x.state !== "waiting") return;
    const was = x.state;
    x.state = "paused"; x.speed = 0;
    if (was === "running" && x.id) {
      F.forget(x.id);
      F.req("cancel", { xfer: x.id }).catch(() => {});
      if (x._stop) await x._stop();
    }
    pump();
  };
  F.resume = (x) => { if (["paused", "error", "waiting"].includes(x.state)) { x.state = "queued"; pump(); } };
  F.cancel = async (x) => {
    await F.pause(x);
    F.xfers = F.xfers.filter((y) => y !== x);
    if (x.opfs) (await opfsDir()).removeEntry(x.opfs).catch(() => {});
    pump();
  };
  F.clearDone = async () => {
    const done = F.xfers.filter((x) => x.state === "done" || x.state === "skipped");
    F.xfers = F.xfers.filter((x) => !done.includes(x));
    const dir = await opfsDir().catch(() => null);
    for (const x of done) if (x.opfs && dir) dir.removeEntry(x.opfs).catch(() => {});
    pump();
  };
  F.active = () => F.xfers.filter((x) => ["running", "verifying", "queued", "waiting", "asking"].includes(x.state));
})();
