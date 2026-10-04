// wado bridge — streaming a video or audio file into a media element, any format.
//
// The daemon (server::files::media) turns the file into fragmented MP4 — copied when the browser
// can decode it, transcoded when not — and this feeds it to Media Source Extensions as it
// arrives. Playback starts after the first fragment; nothing waits for the whole file.
//
//   const s = await W.files.mediaOpen(path, el, { audio, onError, onInfo })
//   s.info (probe: duration, tracks, subs)   s.setAudio(i)   s.destroy()
//
// **Seeking** outside what is buffered restarts the stream at that time. **Pacing** is credit:
// 4 MiB at a time, granted while less than a minute is buffered ahead, so the computer never
// transcodes far ahead of the viewer. **Containment**: every failure goes to `onError` and leaves
// the page alone; `destroy()` stops the daemon's ffmpeg, the MediaSource and every listener.

(() => {
  const F = W.files;
  const CREDIT = 4 << 20;
  const AHEAD = 60; // seconds buffered ahead before credit stops

  F.mediaOpen = async (path, el, opts = {}) => {
    const MS = window.ManagedMediaSource || window.MediaSource;
    if (!MS) throw new Error("This browser cannot stream video (no Media Source Extensions).");
    await F.connect();
    const info = await F.req("probe", { path });
    if (!info.video && !(info.audio || []).length) throw new Error("no video or audio in this file");
    const hevc = MS.isTypeSupported('video/mp4; codecs="hvc1.1.6.L120.90"');

    const ms = new MS();
    if (window.ManagedMediaSource) el.disableRemotePlayback = true;
    const url = URL.createObjectURL(ms);
    el.src = url;
    await new Promise((ok) => ms.addEventListener("sourceopen", ok, { once: true }));
    if (info.duration > 0) ms.duration = info.duration;

    let sb = null, queue = [], xfer = null, gen = 0, ended = false, dead = false;
    let received = 0, granted = 0, audio = opts.audio || 0, mime = "";
    const fail = (why) => { if (dead) return; if (opts.onError) opts.onError(String(why && why.message || why)); };
    const idle = () => new Promise((ok) => { if (!sb || !sb.updating) ok(); else sb.addEventListener("updateend", () => ok(), { once: true }); });
    const bufferedAt = (t) => {
      if (!sb) return 0;
      const b = sb.buffered;
      for (let i = 0; i < b.length; i++) if (t >= b.start(i) - 0.3 && t <= b.end(i)) return b.end(i);
      return 0;
    };

    function pump() {
      if (dead || !sb || sb.updating) return;
      if (!queue.length) {
        if (ended && ms.readyState === "open") { try { ms.endOfStream(); } catch (_) {} }
        return;
      }
      try {
        sb.appendBuffer(queue[0]);
        queue.shift();
      } catch (e) {
        if (e.name !== "QuotaExceededError") return fail(e);
        // Full: drop what is well behind the playhead, and try again once it is gone.
        const behind = el.currentTime - 20;
        if (sb.buffered.length && sb.buffered.start(0) < behind) sb.remove(0, behind);
        else setTimeout(pump, 1000);
      }
    }

    function stopXfer() {
      if (xfer !== null) { F.forget(xfer); F.req("cancel", { xfer }).catch(() => {}); xfer = null; }
    }

    async function start(t) {
      const my = ++gen;
      stopXfer();
      queue = [];
      ended = false;
      if (sb) {
        try { if (sb.updating) sb.abort(); } catch (_) {}
        await idle();
        if (my !== gen || dead) return;
        try { if (sb.buffered.length) { sb.remove(0, Infinity); await idle(); } } catch (_) {}
        if (my !== gen || dead) return;
      }
      received = 0;
      granted = CREDIT;
      xfer = F.stream("stream", { path, start: Math.max(0, t), audio, hevc, credit: CREDIT }, {
        msg(m) {
          if (my !== gen || dead) return true;
          if (m.err) { fail(m.lost ? "the connection to the computer was lost" : m.err); return true; }
          if (m.done) { ended = true; pump(); return true; }
          if (m.mime) {
            if (!sb) {
              if (!MS.isTypeSupported(m.mime)) { fail("this browser cannot play " + m.mime); return true; }
              try {
                sb = ms.addSourceBuffer(m.mime);
                mime = m.mime;
                sb.mode = "segments";
                sb.addEventListener("updateend", pump);
                sb.addEventListener("error", () => fail("the browser could not decode this stream"));
              } catch (e) { fail(e); return true; }
            }
            if (opts.onInfo) opts.onInfo({ mime: m.mime, transcode: m.transcode });
          }
          return false;
        },
        bytes(u8) {
          if (my !== gen || dead) return;
          received += u8.length;
          queue.push(u8);
          pump();
        },
      });
    }

    // Credit while less than AHEAD seconds are buffered.
    const tick = setInterval(() => {
      if (dead || xfer === null || ended) return;
      const ahead = bufferedAt(el.currentTime) - el.currentTime;
      if (ahead < AHEAD && granted - received < CREDIT / 2) {
        granted += CREDIT;
        F.req("credit", { xfer, bytes: CREDIT }).catch(() => {});
      }
    }, 400);

    // A seek outside the buffer restarts the stream there (debounced: a scrub is many seeks).
    let seekTimer = null;
    const onSeek = () => {
      if (dead) return;
      clearTimeout(seekTimer);
      seekTimer = setTimeout(() => { if (!bufferedAt(el.currentTime)) start(el.currentTime); }, 250);
    };
    el.addEventListener("seeking", onSeek);

    await start(opts.start || 0);
    if (opts.start) el.currentTime = opts.start;

    return {
      info,
      get mime() { return mime; },
      setAudio(i) { if (i === audio) return; audio = i; const t = el.currentTime; start(t).then(() => { el.currentTime = t; }); },
      destroy() {
        if (dead) return;
        dead = true;
        gen++;
        stopXfer();
        clearInterval(tick);
        clearTimeout(seekTimer);
        el.removeEventListener("seeking", onSeek);
        try { el.pause(); } catch (_) {}
        el.removeAttribute("src");
        try { el.load(); } catch (_) {}
        URL.revokeObjectURL(url);
      },
    };
  };
})();
