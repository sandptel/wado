// Runnable check for js/video.js — that a stream survives the two ways the stage can not be
// there to receive it.
//
// It exists because both failures are *silent and permanent*: the old code did
// `if (v) v.srcObject = ...` exactly once, so a stream that arrived before the element was
// painted, or was attached to an element a re-render later replaced, was simply gone. The
// session stayed healthy, the daemon kept sending, every metric agreed — and the page was black.
// Reported from the field 2026-09-13 as "it says it continues but does not show up".
//
// Run:  node scripts/video-attach-check.mjs
import { readFileSync } from "node:fs";

const src = readFileSync(new URL("../crates/client/src/js/video.js", import.meta.url), "utf8");

let failures = 0;
const check = (name, got, want) => {
  const ok = JSON.stringify(got) === JSON.stringify(want);
  if (!ok) { failures++; console.log(`FAIL ${name}: got ${JSON.stringify(got)}, want ${JSON.stringify(want)}`); }
  else console.log(`ok   ${name}`);
};

// The smallest DOM that can hold a <video>: one slot, swappable, so "the element was replaced"
// is expressible — which is the whole point of the second case.
function makeWorld() {
  const W = {};
  let el = null;
  const logged = [];
  W.rlog = (l) => logged.push(l);
  const document = { getElementById: (id) => (id === "wado-video" ? el : null) };
  const newVideoEl = () => ({ srcObject: null, play: () => ({ catch: () => {} }) });
  new Function("W", "document", src)(W, document);
  return {
    W, logged,
    mount() { el = newVideoEl(); return el; },
    replace() { el = newVideoEl(); return el; },   // what a Dioxus re-render does
    unmount() { el = null; },
    el: () => el,
  };
}

const STREAM = { id: "stream-1" };

// 1. The reload race: the track arrives before the stage is painted.
{
  const w = makeWorld();
  check("a stream arriving before the stage reports it could not attach", w.W.attachStream(STREAM), false);
  // …and is NOT lost. This is the whole fix: the old code dropped it here, permanently.
  const el = w.mount();
  check("…and attaches as soon as the stage exists", w.W.attachStream(), true);
  check("…to the real element", el.srcObject, STREAM);
}

// 2. The re-render: `session_on` flipping true rebuilds the node above the video, and the new
//    element comes up with a null srcObject while the stream is still arriving.
{
  const w = makeWorld();
  const first = w.mount();
  w.W.attachStream(STREAM);
  check("a stream attaches to the mounted stage", first.srcObject, STREAM);
  const second = w.replace();
  check("a re-render leaves the new element empty", second.srcObject, null);
  check("…and the re-assert notices", w.W.attachStream(), true);
  check("…and puts the same stream back", second.srcObject, STREAM);
}

// 3. Re-asserting when nothing is wrong must be a no-op, because it runs at 1 Hz forever.
{
  const w = makeWorld();
  const el = w.mount();
  w.W.attachStream(STREAM);
  const before = w.logged.length;
  w.W.attachStream();
  w.W.attachStream();
  check("re-asserting an already-attached stream says nothing", w.logged.length - before, 0);
  check("…and leaves it attached", el.srcObject, STREAM);
}

// 4. A stopped session must not be resurrected by the next tick — the timer can outlive the
//    stop by up to a second, and re-attaching a dead stream would be worse than a black page.
{
  const w = makeWorld();
  const el = w.mount();
  w.W.attachStream(STREAM);
  w.W.detachStream();
  check("stopping clears the element", el.srcObject, null);
  check("…and a later re-assert finds nothing to do", w.W.attachStream(), false);
  check("…and does not put it back", el.srcObject, null);
}

// 5. No stream yet at all (a cold page that never started one): the tick must be harmless.
{
  const w = makeWorld();
  w.mount();
  check("a re-assert with no stream is a no-op", w.W.attachStream(), false);
}

console.log(failures ? `\n${failures} FAILED` : "\nall passed");
process.exit(failures ? 1 : 0);
