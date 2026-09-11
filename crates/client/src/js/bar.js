// wado bridge — the control bar's idle visibility.
//
// The bar carries the actions a phone has no keyboard shortcut for, so it has to be reachable
// at all times — and it sits over the video, so it must not be *visible* at all times. It
// fades after a short idle and returns when the user touches near it.
//
// "Near it" rather than "anywhere" on purpose: waking on every interaction would make the bar
// flash back on during ordinary use of the streamed app, which is most of what happens here.

const BAR_IDLE_MS = 2000;
const BAR_ZONE = 0.2; // wake when a press lands in the bottom fifth, where the bar lives

W.bar = {
  timer: null,

  wake() {
    const el = document.getElementById("wado-bar");
    if (!el) return;
    el.classList.remove("idle");
    clearTimeout(W.bar.timer);
    W.bar.timer = setTimeout(() => el.classList.add("idle"), BAR_IDLE_MS);
  },
};

// Capture phase: the video calls preventDefault and stops propagation on its own pointer
// events, so a bubbling listener would never see a press that landed on the stream.
document.addEventListener(
  "pointerdown",
  (e) => {
    if (e.clientY > window.innerHeight * (1 - BAR_ZONE)) W.bar.wake();
  },
  true,
);

// Start the first idle countdown as soon as the bar is mounted, so it begins *visible* and
// fades — a bar that is already hidden when the page loads is a bar nobody finds. Dioxus
// mounts a frame or two after this script runs, hence the wait rather than a direct call.
(function awaitBar() {
  if (document.getElementById("wado-bar")) W.bar.wake();
  else requestAnimationFrame(awaitBar);
})();
