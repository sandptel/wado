// wado bridge — a file's bytes for the viewer, over the files channel, checked against the host's
// SHA-256 like any download; cached for the gallery (the last eight).
//
//   W.files.viewer.bytes(entry, { type, limit, progress, abort }) → Promise<{ url, blob, cut }>
//   W.files.fetchFile(entry)       the whole file as a Blob (sharing it to another app)
//   W.files.viewerForget()         drop the cache (the window closed)
//
// `limit` stops after that many bytes (text): `cut` says so, and the hash is not checked — there
// is no hash of a part. `abort.stop()` cancels and rejects with "stopped".

(() => {
  const F = W.files;
  const V = F.viewer;

  const cache = new Map(); // path@mtime → { url, blob, cut }
  function remember(key, v) {
    cache.set(key, v);
    while (cache.size > 8) { const [k, old] = cache.entries().next().value; URL.revokeObjectURL(old.url); cache.delete(k); }
  }
  F.viewerForget = () => { for (const v of cache.values()) URL.revokeObjectURL(v.url); cache.clear(); };
  V.cached = (e) => cache.has(e.path + "@" + e.mtime);

  V.bytes = (e, { type, limit = 0, progress = () => {}, abort = {} } = {}) => {
    const key = e.path + "@" + e.mtime;
    if (cache.has(key)) return Promise.resolve(cache.get(key));
    return F.connect().then(() => new Promise((ok, no) => {
      const parts = [];
      const sha = W.sha256();
      let got = 0, size = e.size || 0, cut = false;
      const id = F.stream("get", { path: e.path, offset: 0 }, {
        bytes(u8) {
          if (cut) return;
          parts.push(u8); sha.update(u8); got += u8.length;
          progress(got, size);
          if (limit && got >= limit) {
            cut = true;
            F.forget(id);
            F.req("cancel", { xfer: id }).catch(() => {});
            finish();
          }
        },
        msg(m) {
          if (m.err) { no(new Error(m.err)); return true; }
          if (m.done) {
            if (sha.hex() !== m.sha256) { no(new Error("the file arrived damaged — try again")); return true; }
            finish();
            return true;
          }
          if (m.size !== undefined) size = m.size;
          return false;
        },
      });
      abort.stop = () => { F.forget(id); F.req("cancel", { xfer: id }).catch(() => {}); no(new Error("stopped")); };
      function finish() {
        const blob = new Blob(parts, { type: type || V.mime(e.name) });
        const v = { url: URL.createObjectURL(blob), blob, cut };
        remember(key, v);
        ok(v);
      }
    }));
  };

  F.fetchFile = (e) => V.bytes(e, {});
})();
