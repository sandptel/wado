// wado bridge — the selected computer's clipboard history (server::clip), for the Clipboard
// surface (ui/clipboard): its Computer page section and its home widget.
//
// Needs only the relay link. The daemon sends the history on `clip_list` and again whenever the
// computer's clipboard changes. Entries come as previews; the whole of one (`clip_get`) is
// fetched for an image thumbnail or a copy, and kept. Sharing as copies happen (`clip_now`,
// `clip_pushed`) is clip_share.js's.
//
// Copy to this device: a `ClipboardItem` built from a *promise*, created inside the tap — the
// write is then still the tap's, however long the fetch takes. Safari refuses a write that starts
// after an await; this is the shape it accepts.

const clipFull = {};  // id -> { mime, data }
const clipWait = {};  // id -> [resolve]

W.relayOn("clip_history", (m) => emit({
  type: "clipHistory", available: !!m.available, entries: m.entries || [], error: m.error || "",
}));
W.relayOn("clip_data", (m) => {
  clipFull[m.id] = { mime: m.mime, data: m.data };
  if (m.mime.startsWith("image/")) emit({ type: "clipImage", id: m.id, data: m.data });
  (clipWait[m.id] || []).forEach((f) => f(clipFull[m.id]));
  delete clipWait[m.id];
});
W.relayOn("clip_error", (m) => {
  emit({ type: "clipSaid", text: m.message || "The computer's clipboard did not answer.", tone: "bad" });
  emit({ type: "clipError", text: m.message || "" });
});
W.relayOn("clip_now", (m) => W.clipShare.now(W.clipShare.mainKey(), m));
W.relayOn("clip_pushed", (m) => W.clipShare.pushed(W.clipShare.mainKey(), m));

{
  const up = W._relayHandlers.__up;
  W.relayOn("__up", (m) => {
    if (up) up(m);
    W.clipList();
    const k = W.clipShare.mainKey();
    if (k) W.clipShare.linkUp(k);
  });
}

const clipSend = (msg) => { if (W.relayUp && W.relaySendMsg) W.relaySendMsg(msg); };
W.clipList = () => clipSend({ type: "clip_list" });
W.clipPin = (id, on) => clipSend({ type: "clip_pin", id, on: !!on });
W.clipDelete = (id) => { delete clipFull[id]; clipSend({ type: "clip_delete", id }); };

const clipFetch = (id) => {
  if (clipFull[id]) return Promise.resolve(clipFull[id]);
  return new Promise((resolve, reject) => {
    const first = !clipWait[id];
    (clipWait[id] = clipWait[id] || []).push(resolve);
    if (first) clipSend({ type: "clip_get", id });
    setTimeout(() => reject(new Error("the computer did not answer")), 8000);
  });
};

// An image tile came into view: fetch its picture once.
W.clipThumb = (id) => { if (!clipFull[id] && !clipWait[id]) clipFetch(id).catch(() => {}); };
// Entry `id` in full ({ mime, data }), for sending it back to the computer.
W.clipFetchEntry = (id) => clipFetch(id);

// Browsers write only PNG images to the clipboard.
const asPng = async (blob) => {
  if (blob.type === "image/png") return blob;
  const bmp = await createImageBitmap(blob);
  const c = document.createElement("canvas");
  c.width = bmp.width; c.height = bmp.height;
  c.getContext("2d").drawImage(bmp, 0, 0);
  return new Promise((res) => c.toBlob(res, "image/png"));
};

// Inside a tap: entry `id` onto this device's clipboard; `done(ok)` says whether it took.
W.clipCopy = (id, kind, done) => {
  const image = kind === "image";
  const fin = done || (() => {});
  if (!navigator.clipboard || !window.ClipboardItem) return fin(false);
  const blob = clipFetch(id).then(async (c) => image
    ? asPng(await (await fetch(c.data)).blob())
    : new Blob([c.data], { type: "text/plain" }));
  navigator.clipboard
    .write([new ClipboardItem({ [image ? "image/png" : "text/plain"]: blob })])
    .then(() => fin(true), () => fin(false));
};

// Tiles glide to their new place when the list reorders — a pin lifting one to the top, a new
// copy pushing the rest down, a delete closing the gap, a filter. FLIP: remember where each tile
// was, and after the DOM changes animate it from there to where it is now. Installed once per
// list element (Dioxus `onmounted`).
// Every clipboard list on the page (a Computer page's, each Clipboard widget's), once each.
W.clipAnimate = () => document.querySelectorAll(".cliplist").forEach(clipFlip);
const reduced = () => document.documentElement.dataset.motion === "off" ||
  matchMedia("(prefers-reduced-motion: reduce)").matches;
function clipFlip(list) {
  if (list._flip) return;
  const tops = new Map();
  const snap = () => { for (const el of list.children) if (el.dataset.id) tops.set(el.dataset.id, el.offsetTop); };
  snap();
  list._flip = new MutationObserver(() => {
    for (const el of list.children) {
      const was = tops.get(el.dataset.id);
      const dy = was == null ? 0 : was - el.offsetTop;
      if (dy && !reduced()) el.animate([{ translate: `0 ${dy}px` }, { translate: "0 0" }],
        { duration: 380, easing: "cubic-bezier(.32,.72,0,1)" });
    }
    snap();
  });
  list._flip.observe(list, { childList: true });
  // Heights also change without a DOM change — a deleted tile collapsing, a picture arriving —
  // and the next move must start from where things are after that, not before.
  list.addEventListener("transitionend", snap);
  list.addEventListener("load", snap, true);
}
