// wado bridge — ghost cursors: where the other devices in this session last pointed, as
// labelled dots over the picture (Decision Log 2026-10-30). The compositor still draws no
// cursor; these are the page's own, from the daemon's `wado-presence` data channel — unordered,
// no retransmits, at most 20 a second per device, never this device's own.
//
// Also the participants chip: a small pill in the top-right corner while more than one device is
// in, so nobody is watched without knowing it.
//
//   W.presenceChannel(pc)       open the channel on a new peer connection (relay.js)
//   W.presenceRoster(list, you) who is in (multi.js), for the chip and the dots' names
//   W.presenceClear()           the session is over for this page

const PRESENCE_LABEL = "wado-presence";
const PRESENCE_IDLE_MS = 3000;   // a dot nobody has moved for this long dims
let presenceLayer = null;
let presenceChip = null;
const presenceDots = new Map(); // id -> {el, x, y, at}

function presenceRoot() {
  if (presenceLayer && presenceLayer.isConnected) return presenceLayer;
  presenceLayer = document.createElement("div");
  presenceLayer.id = "wado-presence";
  presenceLayer.setAttribute("aria-hidden", "true");
  document.body.appendChild(presenceLayer);
  return presenceLayer;
}

// The video's content rect (object-fit: contain), as input_core.js normalizes against.
function presenceRect() {
  const v = document.getElementById("wado-video");
  if (!v || !v.videoWidth || !v.videoHeight) return W.fixture ? { x: 0, y: 0, w: innerWidth, h: innerHeight } : null;
  const r = v.getBoundingClientRect();
  const s = Math.min(r.width / v.videoWidth, r.height / v.videoHeight);
  const w = v.videoWidth * s, h = v.videoHeight * s;
  return { x: r.left + (r.width - w) / 2, y: r.top + (r.height - h) / 2, w, h };
}

function presencePlace(d) {
  const r = presenceRect();
  if (!r) return;
  d.el.style.translate = (r.x + d.x * r.w).toFixed(1) + "px " + (r.y + d.y * r.h).toFixed(1) + "px";
}

function presenceShow(p) {
  let d = presenceDots.get(p.id);
  if (!p.kind) {
    // Left: the dot shrinks away, then goes.
    if (d) {
      d.el.classList.add("gone");
      setTimeout(() => d.el.remove(), 260);
      presenceDots.delete(p.id);
    }
    return;
  }
  if (!d) {
    const el = document.createElement("div");
    el.className = "ghost";
    el.style.setProperty("--hue", "var(--base0" + "89ABCDEF"[p.hue % 8] + ")");
    const dot = document.createElement("i");
    const label = document.createElement("span");
    label.textContent = p.name || "device";
    el.append(dot, label);
    presenceRoot().appendChild(el);
    d = { el, x: p.x, y: p.y, at: 0 };
    presenceDots.set(p.id, d);
    // Placed before it is shown, so it pops in where it is rather than flying from the corner.
    presencePlace(d);
    requestAnimationFrame(() => el.classList.add("in"));
  }
  d.x = p.x; d.y = p.y; d.at = Date.now();
  d.el.classList.toggle("touch", p.kind === "touch");
  d.el.classList.remove("idle");
  presencePlace(d);
}

W.presenceChannel = (pc) => {
  let dc;
  try { dc = pc.createDataChannel(PRESENCE_LABEL, { ordered: false, maxRetransmits: 0 }); }
  catch (_) { return; }
  dc.onmessage = (e) => {
    let m;
    try { m = JSON.parse(e.data); } catch (_) { return; }
    for (const p of m.p || []) presenceShow(p);
  };
};

W.presenceRoster = (list, you) => {
  const others = (list || []).filter((p) => p.id !== you);
  // Dots of devices no longer in it.
  for (const [id, d] of presenceDots) {
    if (!others.some((p) => p.id === id)) { d.el.remove(); presenceDots.delete(id); }
  }
  if (!others.length) {
    if (presenceChip) { presenceChip.classList.remove("in"); }
    return;
  }
  if (!presenceChip || !presenceChip.isConnected) {
    presenceChip = document.createElement("div");
    presenceChip.id = "wado-here";
    presenceChip.setAttribute("role", "status");
    document.body.appendChild(presenceChip);
  }
  const names = others.map((p) => p.name).join(", ");
  presenceChip.replaceChildren(
    Object.assign(document.createElement("i"), { className: "pulse" }),
    Object.assign(document.createElement("b"), { textContent: String(list.length) }),
    Object.assign(document.createElement("span"), { textContent: "here · " + names }),
  );
  presenceChip.setAttribute("aria-label", list.length + " devices in this session: you and " + names);
  requestAnimationFrame(() => presenceChip.classList.add("in"));
};

W.presenceClear = () => {
  for (const d of presenceDots.values()) d.el.remove();
  presenceDots.clear();
  if (presenceChip) presenceChip.classList.remove("in");
};

// Idle dots dim; every dot follows the picture when the page resizes or turns.
setInterval(() => {
  const now = Date.now();
  for (const d of presenceDots.values()) {
    if (now - d.at > PRESENCE_IDLE_MS) d.el.classList.add("idle");
  }
}, 1000);
W.presenceRefit = () => { for (const d of presenceDots.values()) presencePlace(d); };
window.addEventListener("resize", W.presenceRefit);

// Screenshot and UI checks (`?fixture`): play the daemon's side of presence.
if (W.fixture) W.fixturePresence = (batch, list, you) => {
  if (list) W.presenceRoster(list, you);
  for (const p of batch || []) presenceShow(p);
};
