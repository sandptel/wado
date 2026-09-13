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
  if (stream) W._stream = stream;
  if (!W._stream) return false;
  const v = document.getElementById("wado-video");
  if (!v) return false;
  if (v.srcObject === W._stream) return true;
  v.srcObject = W._stream;
  // A freshly built element starts paused: `autoplay` covers the first attach, this covers a
  // re-attach. Muted and playsinline are set on the element, so no user gesture is needed and a
  // rejection here is only ever a race with another play().
  if (v.play) { try { const p = v.play(); if (p && p.catch) p.catch(() => {}); } catch (_) {} }
  if (W.rlog) W.rlog("video: stream attached to the stage");
  return true;
};

/// Session over: forget the stream so a later re-assert cannot resurrect a dead one.
W.detachStream = () => {
  W._stream = null;
  const v = document.getElementById("wado-video");
  if (v) v.srcObject = null;
};
