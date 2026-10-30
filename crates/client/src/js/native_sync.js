// wado bridge — what the Android app mirrors from this page (native.js; nothing without it).
//
//   page → app  theme       {hexes: [16 × "rrggbb"]}       the scheme in use, for the app's own screens
//   page → app  computers   {profiles: [{key, name, host, relay_url, remote_id, conn_mode}], selected}
//   page → app  nowPlaying  {bus, app, title, artist, art, playing, position_ms, length_ms,
//                            can_next, can_prev, can_seek, volume, muted, sink}   or {} when nothing plays
//   page → app  clip        {text, sensitive}                the computer copied something
//   app → page  media       {op: "play_pause"|"next"|"previous"|"seek_to", ms?}
//   app → page  volume      {volume: 0..1}  (or {delta: ±0.05})
//   app → page  clip        {text}            the phone's clipboard, when the app came to the front
//
// Each hook wraps a function or chains a relay handler that an earlier file defined, so this
// file loads after all of them (bridge.rs).

(() => {
  const N = W.native;
  if (!N) return;

  // ── theme ────────────────────────────────────────────────────────────────────────────
  const themeNow = () => {
    const st = document.documentElement.style;
    const hexes = [];
    for (let i = 0; i < 16; i++) {
      const v = st.getPropertyValue("--base0" + i.toString(16).toUpperCase()).trim().replace(/^#/, "");
      if (!/^[0-9a-fA-F]{6}$/.test(v)) return null;
      hexes.push(v.toLowerCase());
    }
    return hexes;
  };
  const apply = W.applyTheme;
  W.applyTheme = (hexes) => {
    const ok = apply(hexes);
    if (ok) N.send("theme", { hexes: hexes.map((h) => String(h).toLowerCase()) });
    return ok;
  };
  const t0 = themeNow();
  if (t0) N.send("theme", { hexes: t0 });

  // ── computers ────────────────────────────────────────────────────────────────────────
  // Only what the app's home and share picker show; never a key or a pin.
  const computers = (s) => {
    const list = Array.isArray(s && s.profiles) ? s.profiles : [];
    return {
      profiles: list.map((p) => ({
        key: (p.relay_url || "") + "|" + (p.remote_id || ""),
        name: p.name || "", host: p.host || "", relay_url: p.relay_url || "",
        remote_id: p.remote_id || "", conn_mode: p.conn_mode || "",
      })),
      selected: (s && s.profile) | 0,
    };
  };
  const save = W.saveSettings;
  W.saveSettings = (obj) => {
    save(obj);
    N.send("computers", computers(obj));
  };
  N.send("computers", computers(W.loadSettings()));

  // ── now playing + volume (the computer's, from server::host) ─────────────────────────
  let player = null, sink = null;
  const prevHost = W._relayHandlers.host_state;
  W.relayOn("host_state", (msg) => {
    if (prevHost) prevHost(msg);
    const st = W.hostState || {};
    const media = st.media || [];
    player = media.find((p) => p.playing) || media[0] || null;
    const a = st.audio || {};
    sink = (a.sinks || []).find((s) => s.name === a.default_sink) || null;
    if (!player) { N.send("nowPlaying", {}); return; }
    N.send("nowPlaying", {
      bus: player.bus, app: player.app || "", title: player.title || "", artist: player.artist || "",
      art: player.art || "", playing: !!player.playing, position_ms: player.position_ms || 0,
      length_ms: player.length_ms || 0, can_next: !!player.can_next, can_prev: !!player.can_prev,
      can_seek: !!player.can_seek,
      volume: sink ? sink.volume : -1, muted: sink ? !!sink.muted : false, sink: sink ? sink.label : "",
    });
  });
  N.on("media", (m) => {
    if (!player || !W.hostDo) return;
    const op = m.op === "seek_to" ? { op: "seek_to", ms: Math.max(0, m.ms | 0) } : { op: String(m.op || "") };
    W.hostDo({ do: "media", bus: player.bus, op });
  });
  N.on("volume", (m) => {
    if (!sink || !W.hostDo) return;
    const v = typeof m.volume === "number" ? m.volume : sink.volume + (Number(m.delta) || 0);
    sink.volume = Math.max(0, Math.min(1, v));
    W.hostDo({ do: "sink_volume", id: sink.id, volume: sink.volume });
  });

  // ── clipboard ────────────────────────────────────────────────────────────────────────
  // The computer copied: the app puts it on the phone's clipboard (marked sensitive when the
  // computer says so), focused or not. The page still keeps it in the history as before.
  const prevClip = W._relayHandlers.clipboard;
  W.relayOn("clipboard", (msg) => {
    if (prevClip) prevClip(msg);
    if (msg.text) N.send("clip", { text: String(msg.text), sensitive: !!msg.sensitive });
  });
  // The phone copied, and the app came to the front: hand it to the computer. The app has
  // already left out anything marked sensitive and anything it put there itself.
  // With the shared clipboard (clip_share.js) it goes to every computer this phone shares with,
  // named by its hash so it is not sent twice; the session-only path is the fallback.
  N.on("clip", (m) => {
    if (!m.text) return;
    if (W.clipShare && W.clipShare.fromApp) W.clipShare.fromApp(String(m.text));
    else if (W.relayUp && W.clipboardSend) W.clipboardSend(String(m.text));
  });
})();
