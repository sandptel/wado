// wado bridge — the session's sound. Its own <audio> element rather than the video's: the
// video stays muted, which is what lets it autoplay; sound needs a gesture on most phones,
// and Start is one (W.audioUnlock is called from it), as is the sound tile.

W.audio = (() => {
  const el = document.createElement("audio");
  el.autoplay = true;
  el.setAttribute("playsinline", "");
  document.body.appendChild(el);
  let volume = 1, muted = false;
  const apply = () => { el.volume = volume; el.muted = muted; };
  return {
    attach(track) {
      el.srcObject = new MediaStream([track]);
      apply();
      el.play().catch(() => emit({ type: "audioBlocked" }));
    },
    unlock() { apply(); if (el.srcObject) el.play().catch(() => {}); },
    set(v, m) { volume = Math.min(1, Math.max(0, v)); muted = !!m; apply(); },
  };
})();
W.setAudio = (volume, muted) => W.audio.set(volume, muted);
W.audioUnlock = () => W.audio.unlock();
