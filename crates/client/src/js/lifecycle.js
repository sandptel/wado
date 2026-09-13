// wado bridge — session lifecycle (start / stop) + page-lifetime keep-alive.
// Launch and the window actions live in control.js; this file only starts and ends sessions.
// MUST be concatenated last: the trailing never-resolving await keeps this eval (and its
// dioxus.send channel) alive for the app's lifetime.
//
// Supports two connection modes:
//   Direct mode  — POST /session/start to server, then HTTP /offer for WebRTC.
//   Relay mode   — all control + WebRTC signaling goes through wado-relay WS.
//                  relayOpts = { relayUrl, remoteId } enables relay mode.

W.start = async (server, config, relayOpts) => {
  W.server = server;
  // The scroll paths convert CSS pixels to the session's logical pixels and need this scale
  // to do it. Taken from the config the UI already hands us rather than plumbed separately.
  W.outputScale = config && config.scale > 0 ? config.scale : 1;
  // Before either branch: a session with no touch input for minutes would otherwise let the
  // phone sleep, and the resulting pagehide tears the session down.
  W.wake.acquire();

  if (relayOpts && relayOpts.relayUrl) {
    // ── Relay mode ────────────────────────────────────────────────────────────
    status("relay: connecting…");
    try {
      W.reconnectAttempts = 0;
      await W.relayConnect(relayOpts.relayUrl, relayOpts.remoteId, config);
    } catch (e) {
      status("relay error: " + (e && e.message ? e.message : String(e)));
      emit({ type: "startFailed" });
    }
  } else {
    // ── Direct mode (existing behaviour) ─────────────────────────────────────
    status("starting session…");
    try {
      const res = await fetch(server + "/session/start", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(config),
      });
      if (!res.ok) {
        status("start failed: " + (await res.text()));
        emit({ type: "startFailed" });
        return;
      }
      try {
        const info = await res.json();
        if (info && info.encoder && info.encoder.mode) {
          emit({ type: "encoder", mode: info.encoder.mode, pipeline: info.encoder.pipeline || "" });
        }
      } catch (_) {}
      W.sessionOn = true;
      W.reconnectAttempts = 0;
      stagebar("Session running — connecting video…");
      await W.connectWebRTC();
    } catch (e) {
      status("error: " + (e && e.message ? e.message : e));
      emit({ type: "startFailed" });
    }
  }
};

// Apply settings to a running session. Relay mode only for now: the direct HTTP transport has
// no route for it, and relay is the path a phone actually uses.
//
// ponytail: no direct-mode branch. Add `POST /session/reconfigure` when something needs it.
W.reconfigure = (config) => {
  W.outputScale = config && config.scale > 0 ? config.scale : 1;
  if (W.relayMode) return W.relayReconfigure(config);
  status("applying settings needs relay mode");
  return false;
};

// Tell the daemon when the page goes off screen and when it comes back.
//
// This is the cheapest large win in the whole stack: a hidden tab still holds a live peer
// connection and still receives RTP, and the browser discards all of it. Without this the daemon
// renders, encodes and transmits the full stream to something nobody can see — measured
// 2026-09-13 at 11.9 Mbps out and 65 kbps reaching the decoder, on mobile data.
//
// `visibilitychange` and not `pagehide`: pagehide means the page may be going away, which is a
// different question and one this project has already got wrong once (see the note above).
document.addEventListener("visibilitychange", () => {
  if (!W.sessionOn || !W.relayMode) return;
  const visible = !document.hidden;
  if (W.rlog) W.rlog("page is now " + (visible ? "visible" : "hidden"));
  if (W.relayVisible) W.relayVisible(visible);
});

W.stopSession = async () => {
  W.sessionOn = false;
  W.wake.release();
  W.stopStats();
  W.latency.stop();
  W.resetInput();
  W.inputDC = null;
  W.motionDC = null;
  if (W.pc) { try { W.pc.close(); } catch (_) {} W.pc = null; }
  const v = document.getElementById("wado-video");
  if (v) v.srcObject = null;
  stagebar("No session.");

  if (W.relayMode) {
    W.relayStop();
  } else {
    try { await fetch(W.server + "/session/stop", { method: "POST" }); } catch (_) {}
  }
};

// Free server resources promptly when the page is really going away.
//
// **Relay mode sends nothing here.** `pagehide` does not mean "closing" on a phone: it fires on
// an app switch, a pulled-down shade, a screen lock — and it fired on a page that then kept its
// WebRTC connection and its data channel alive. Observed 2026-09-12 21:46:53: the session was
// destroyed, Chrome and every window with it, while the viewer went on swiping into a live input
// channel with nothing behind it (`input dropped — no active session`, for ten seconds).
//
// `event.persisted` is *supposed* to distinguish the two — true means the back/forward cache, so
// the page is expected to return — but bfcache eligibility is revoked by an open WebSocket or
// WebRTC connection in several Chromium versions, and this page has both. So the flag may well
// read `false` on the very app switch it is meant to identify. It is logged rather than trusted;
// the next app switch will say what this phone actually reports.
//
// What replaces it is `viewer_watchdog` in `crates/server/src/relay_client.rs`, which stops a
// session after VIEWER_GRACE of relay silence with no WebRTC. That is browser-independent and
// covers a page that dies without ever reaching this handler — which is the case this beacon was
// written for, back when no watchdog existed. Cost: a deliberately-closed tab holds its session
// for up to the grace period. Same trade-off already recorded in `issues.md` I13.
//
// **Direct mode still sends the beacon.** The watchdog lives in the relay client, so the direct
// path has no equivalent net and this is its only cleanup.
window.addEventListener("pagehide", (ev) => {
  if (!W.sessionOn) return;
  if (W.rlog) W.rlog("pagehide persisted=" + ev.persisted + " relayMode=" + !!W.relayMode);
  if (W.relayMode) return;
  if (W.server) navigator.sendBeacon(W.server + "/session/stop");
});

// Keep this eval (and its `dioxus` send channel) alive for the app's lifetime.
await new Promise(() => {});
