// wado bridge — the computer's clipboard history (server::clip), for the landing page's rail.
//
// Needs only the relay link. The daemon sends the history on `clip_list` and again whenever the
// computer's clipboard changes. Entries come as previews; the whole of one (`clip_get`) is
// fetched for an image thumbnail or a copy, and kept.
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
  emit({ type: "captureNote", text: m.message || "The computer's clipboard did not answer." });
  emit({ type: "clipError", text: m.message || "" });
});

{
  const up = W._relayHandlers.__up;
  W.relayOn("__up", (m) => { if (up) up(m); W.clipList(); });
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

// Browsers write only PNG images to the clipboard.
const asPng = async (blob) => {
  if (blob.type === "image/png") return blob;
  const bmp = await createImageBitmap(blob);
  const c = document.createElement("canvas");
  c.width = bmp.width; c.height = bmp.height;
  c.getContext("2d").drawImage(bmp, 0, 0);
  return new Promise((res) => c.toBlob(res, "image/png"));
};

// Inside a tap: entry `id` onto this device's clipboard.
W.clipCopy = (id, kind) => {
  const image = kind === "image";
  const note = (text) => emit({ type: "captureNote", text });
  if (!navigator.clipboard || !window.ClipboardItem) {
    note("This browser has no clipboard to write to.");
    return;
  }
  const blob = clipFetch(id).then(async (c) => image
    ? asPng(await (await fetch(c.data)).blob())
    : new Blob([c.data], { type: "text/plain" }));
  navigator.clipboard
    .write([new ClipboardItem({ [image ? "image/png" : "text/plain"]: blob })])
    .then(() => note(image ? "Image copied to this device." : "Copied to this device."))
    .catch(() => note("This browser would not take it — tap the tile again."));
};

const asDataUrl = (blob) => new Promise((res, rej) => {
  const r = new FileReader();
  r.onload = () => res(r.result);
  r.onerror = () => rej(r.error);
  r.readAsDataURL(blob);
});

const CLIP_MAX = 12 * 1024 * 1024;

// Inside a tap: this device's clipboard onto the computer's (and into the session).
W.clipPaste = async () => {
  const note = (text) => emit({ type: "captureNote", text });
  const push = (mime, data) => {
    clipSend({ type: "clip_push", mime, data });
    note("Sent to the computer's clipboard.");
  };
  try {
    if (navigator.clipboard && navigator.clipboard.read) {
      for (const item of await navigator.clipboard.read()) {
        const img = item.types.find((t) => t.startsWith("image/"));
        if (img) {
          const b = await item.getType(img);
          if (b.size > CLIP_MAX) { note("That image is too big to send."); return; }
          push(b.type || img, await asDataUrl(b));
          return;
        }
        if (item.types.includes("text/plain")) {
          const t = await (await item.getType("text/plain")).text();
          if (t) { push("text/plain", t); return; }
        }
      }
    } else if (navigator.clipboard && navigator.clipboard.readText) {
      const t = await navigator.clipboard.readText();
      if (t) { push("text/plain", t); return; }
    }
    note("This device's clipboard is empty.");
  } catch (_) {
    note("This browser would not share its clipboard.");
  }
};

// Tiles glide to their new place when the list reorders — a pin lifting one to the top, a new
// copy pushing the rest down, a delete closing the gap, a filter. FLIP: remember where each tile
// was, and after the DOM changes animate it from there to where it is now. Installed once per
// list element (Dioxus `onmounted`).
W.clipAnimate = () => {
  const list = document.querySelector("#cliprail .cliplist");
  if (!list || list._flip) return;
  const tops = new Map();
  const snap = () => { for (const el of list.children) if (el.dataset.id) tops.set(el.dataset.id, el.offsetTop); };
  snap();
  list._flip = new MutationObserver(() => {
    for (const el of list.children) {
      const was = tops.get(el.dataset.id);
      const dy = was == null ? 0 : was - el.offsetTop;
      if (dy) el.animate([{ translate: `0 ${dy}px` }, { translate: "0 0" }],
        { duration: 380, easing: "cubic-bezier(.32,.72,0,1)" });
    }
    snap();
  });
  list._flip.observe(list, { childList: true });
  // Heights also change without a DOM change — a deleted tile collapsing, a picture arriving —
  // and the next move must start from where things are after that, not before.
  list.addEventListener("transitionend", snap);
  list.addEventListener("load", snap, true);
};
