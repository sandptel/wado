// wado bridge — screenshot and recording, from the stream this page is already receiving.
//
// Nothing goes to the daemon: the <video> element holds the decoded frames at the stream's
// own resolution, so a screenshot is one canvas draw and a recording is a MediaRecorder on
// the same MediaStream. The files are saved on the device doing the viewing — the phone.

const stamp = () => new Date().toISOString().replace(/[:T]/g, "-").slice(0, 19);

function save(blob, name) {
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = name;
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 10000);
}

W.screenshot = () => {
  const v = W.videoEl || document.getElementById("wado-video");
  if (!v || !v.videoWidth) { emit({ type: "captureNote", text: "No picture to capture yet." }); return; }
  const c = document.createElement("canvas");
  c.width = v.videoWidth;
  c.height = v.videoHeight;
  c.getContext("2d").drawImage(v, 0, 0);
  c.toBlob((b) => {
    if (b) { save(b, `wado-${stamp()}.png`); emit({ type: "captureNote", text: `Screenshot saved — ${c.width}×${c.height}` }); }
  }, "image/png");
};

let rec = null;
W.recordToggle = () => {
  if (rec) { rec.stop(); return; }
  const v = W.videoEl || document.getElementById("wado-video");
  const stream = v && v.srcObject;
  if (!stream || typeof MediaRecorder === "undefined") {
    emit({ type: "captureNote", text: "This browser cannot record the stream." });
    return;
  }
  // The first container this browser can write: WebM on Chrome/Firefox, MP4 on Safari.
  const type = ["video/webm;codecs=vp9", "video/webm;codecs=vp8", "video/webm", "video/mp4"]
    .find((t) => MediaRecorder.isTypeSupported(t)) || "";
  const chunks = [];
  try { rec = new MediaRecorder(stream, type ? { mimeType: type } : {}); } catch (e) {
    emit({ type: "captureNote", text: "Recording failed to start: " + e.message });
    return;
  }
  rec.ondataavailable = (e) => { if (e.data && e.data.size) chunks.push(e.data); };
  rec.onstop = () => {
    const ext = (rec.mimeType || type).includes("mp4") ? "mp4" : "webm";
    save(new Blob(chunks, { type: rec.mimeType || type }), `wado-${stamp()}.${ext}`);
    rec = null;
    emit({ type: "recording", on: false });
    emit({ type: "captureNote", text: "Recording saved." });
  };
  rec.start(1000);
  emit({ type: "recording", on: true });
};
