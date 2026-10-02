// wado bridge — the <video> element's stream, and keeping the two attached.
//
// Why this is a file and not two lines inside each `ontrack`.
//
// The stream used to be attached exactly once, by whichever `ontrack` fired:
//
//     const v = document.getElementById("wado-video");
//     if (v) v.srcObject = ev.streams[0];
//
// `if (v)` is a silent failure, and there are two ways to hit it — both of them reported from
// the field on 2026-09-13 as *"the stream says it continues and maybe it does, but it does not
// show up on the website"*:
//
//   * **The element may not exist yet.** On a reload that resumes a running session the relay
//     link dials at load, the daemon answers `session_started` in tens of milliseconds, and
//     `ontrack` can land before Dioxus has painted the stage. The stream is dropped on the floor
//     and *nothing ever retries* — so the stage bar says "Streaming (relay)", the daemon logs
//     "track received — media is flowing", and the page stays black. Every number agrees the
//     session is healthy, because it is; only the picture is missing.
//   * **The element may be replaced.** The stage is declarative: `session_on` flipping true adds
//     the encoder badge and the software-encode banner *above* the video, and a re-render that
//     rebuilds that node takes `srcObject` with it. Same symptom, different instant.
//
// So the stream is kept here, attaching is idempotent, and `stats.js` re-asserts it on its 1 Hz
// tick. Re-asserting costs an identity check when it is already right, and it is the only thing
// that recovers either case without a human pressing something.

W._stream = null;

/// Attach `stream` (or whatever was last seen) to the stage, if the element is there to take it.
/// Safe to call at any time, from anywhere, as often as you like.
W.attachStream = (stream) => {
  // Video tracks only. The session's sound rides on the same stream now (server::audio) and is
  // played by its own <audio> element; left on the <video>, it makes the element "audible",
  // and an audible element is refused autoplay — which is how the picture once sat paused on a
  // black frame while every frame was arriving and decoding fine.
  if (stream) W._stream = new MediaStream(stream.getVideoTracks());
  if (!W._stream) return false;
  const v = document.getElementById("wado-video");
  if (!v) return false;
  // As properties, not just attributes: a framework-set `muted` attribute is not the live
  // `muted` state the autoplay policy reads.
  v.muted = true;
  v.defaultMuted = true;
  v.playsInline = true;
  if (v.srcObject !== W._stream) {
    v.srcObject = W._stream;
    if (W.rlog) W.rlog("video: stream attached to the stage");
  }
  W.videoPlay();
  return true;
};

// Play, and if the browser still says no, play on the very next tap anywhere — never leave the
// picture paused with nothing on screen saying why.
W.videoPlay = () => {
  const v = document.getElementById("wado-video");
  if (!v || !v.srcObject || !v.paused) return;
  let p;
  try { p = v.play(); } catch (_) { return; }
  if (p && p.catch) p.catch((e) => {
    if (W.rlog) W.rlog("video: play() refused (" + (e && e.name) + ") — will start on the next tap");
    emit({ type: "captureNote", text: "Tap the screen to start the picture." });
    const once = () => { document.removeEventListener("pointerdown", once, true); W.videoPlay(); };
    document.addEventListener("pointerdown", once, true);
  });
};

// A session whose picture is paused is a fault, whatever paused it (a tab switch, the OS
// reclaiming the decoder): checked every two seconds and restarted.
setInterval(() => { if (W.sessionOn) W.videoPlay(); }, 2000);

/// Session over: forget the stream so a later re-assert cannot resurrect a dead one.
W.detachStream = () => {
  W._stream = null;
  const v = document.getElementById("wado-video");
  if (v) v.srcObject = null;
};
