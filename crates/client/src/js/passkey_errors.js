// wado bridge — what went wrong with a passkey, in plain words, and what to do about it.
//
//   W.passkeyWhy(err, {purpose, timedOut, computer}) → { note, kind }      (pure; tested)
//
// `err` is what navigator.credentials rejected with — a browser's DOMException, or the Android
// app's own authenticator's answer (js/native_passkey.js), which carries `wadoCode` and words
// already meant for a person. `kind` is what the sheet offers next: "retry" (Try again),
// "elsewhere" (this passkey cannot work here: approve on the computer, or add one here),
// "setup" (the phone needs a screen lock or a fingerprint first), "silent" (the person
// cancelled on purpose — say nothing alarming).

(() => {
  // The app's authenticator says exactly what happened (passkey/AuthErrors.kt).
  const NATIVE = {
    cancelled: "silent",
    timeout: "retry",
    lockout: "retry",
    lockout_permanent: "setup",
    no_screen_lock: "setup",
    none_enrolled: "setup",
    hardware: "retry",
    not_held: "elsewhere",
    invalidated: "elsewhere",
    busy: "retry",
    failed: "retry",
  };

  W.passkeyWhy = (e, o) => {
    o = o || {};
    const name = (e && e.name) || "";
    const msg = String((e && e.message) || e || "");
    const comp = o.computer || "the computer";
    if (o.timedOut) {
      return {
        kind: "retry",
        note: "Your passkey didn't answer in time. Try again — or approve this phone on " + comp + " instead.",
      };
    }
    if (e && e.wadoCode) {
      return { kind: NATIVE[e.wadoCode] || "retry", note: msg || "The passkey didn't work." };
    }
    if (name === "AbortError") return { kind: "silent", note: "" };
    if (name === "InvalidStateError" && o.purpose === "register") {
      return {
        kind: "elsewhere",
        note: "This phone already holds a passkey for wado. Use it to unlock, or remove the old one on " + comp + " with `wado passkey remove`.",
      };
    }
    if (name === "NotAllowedError") {
      // Chrome, Safari and Firefox say the same thing for "cancelled", "no passkey here
      // matches", "the screen lock failed" and "timed out": a privacy rule. Say all of it.
      return {
        kind: "retry",
        note: o.purpose === "register"
          ? "The passkey wasn't made — cancelled, or the screen lock wasn't confirmed. Try again."
          : "Cancelled, or this phone has no passkey for " + comp + " here (one made in another browser or app can't unlock this one). Try again, or approve this phone on " + comp + ".",
      };
    }
    if (name === "OperationError" || /already pending/i.test(msg)) {
      return { kind: "retry", note: "Another passkey prompt was still open. Try again." };
    }
    if (name === "SecurityError") {
      return { kind: "elsewhere", note: "This page's address can't hold a passkey — open wado over https." };
    }
    if (name === "NotSupportedError") {
      return {
        kind: "setup",
        note: "This browser can't make a passkey with a screen lock. Set a screen lock (PIN, fingerprint or face) on this device, or use Chrome or Safari.",
      };
    }
    return { kind: "retry", note: "The passkey didn't work: " + msg };
  };
})();
