// wado bridge — the clipboard, both ways, as text.
//
// Session → phone: an app copies, the daemon sends `clipboard`. It is written to the phone's
// clipboard when the browser allows it — focused page, and on Safari not even then without a
// gesture — and always kept in the control centre's history with a Copy button, which is a
// gesture and therefore always works.
//
// Phone → session: reading the phone's clipboard needs a gesture too, so it happens only on a
// tap (the Clipboard tile, or "Paste here" in the history).

W.relayOn("clipboard", (msg) => {
  const text = msg.text || "";
  if (!text) return;
  emit({ type: "clipboard", text });
  if (document.hasFocus() && navigator.clipboard && navigator.clipboard.writeText) {
    navigator.clipboard.writeText(text).catch(() => {});
  }
});

W.clipboardSend = (text) => {
  if (text) W.relaySendMsg({ type: "clipboard_set", text: String(text) });
};

// Inside a tap: read the phone's clipboard and hand it to the session.
W.clipboardFromPhone = async () => {
  try {
    const text = await navigator.clipboard.readText();
    if (text) { W.clipboardSend(text); emit({ type: "clipboardSent", text }); }
  } catch (_) {
    emit({ type: "clipboardSent", text: "", error: "This browser would not share its clipboard." });
  }
};

// Inside a tap: put a history entry on the phone's clipboard.
W.clipboardToPhone = (text) => {
  if (navigator.clipboard && navigator.clipboard.writeText) navigator.clipboard.writeText(text).catch(() => {});
};
