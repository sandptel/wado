// wado bridge — the passkey step-up (daemon: crates/server/src/passkey/). Shells, writing files
// and the trackpad/keyboard need a fresh unlock with this device's passkey, on top of its grants.
//
//   W.passkeyThen(what, fn?)  run `fn` (or tell Rust {go: what}) now if unlocked, else ask for
//                             the passkey first and run it once unlocked. `what` names the door:
//                             shell | pad | files | allow | claude | new | add | paired | again
//   W.passkeyGo()             the sheet's button: unlock, or add a passkey if there is none
//   W.passkeyAdd()            add (or, while unlocked, replace) this device's passkey
//   W.passkeyFor(what, fn)    straight into adding or unlocking for an open door, then `fn`
//   W.passkeyCancel()         the sheet was dismissed
//   W.passkeyLockNow()        lock at once
//   W.passkeyStay(stay, leave?) how long an unlock lasts here — "15m" | "1h" | "8h" | "lock" —
//                             and whether leaving the app locks it; only while unlocked
//   W.passkeyIsLock(text)     is this one of the daemon's passkey refusals?
//
// It locks itself after the daemon's idle window without the person touching the app, and the
// moment the app goes to the background — except while a picker this page opened is up (the
// passkey prompt itself, a file chooser), which hides the page without anyone leaving it.
//
// To Rust: {type:"passkey", state, busy, need, note, supported, platform, go}. `need` set means
// "show the sheet for this door"; `go` names a door to open now that it is unlocked.

(() => {
  const P = (W.passkey = {
    state: { registered: false, unlocked: false, idle_left_s: 0, idle_s: 900, error: "", origin: "" },
    busy: false,
    need: "",
    note: "",
    supported: !!window.PublicKeyCredential,
    platform: false,
  });
  let then = null;          // what to run once unlocked
  let lastAct = Date.now(); // the person last touched the app
  let lastKeep = 0;         // the daemon last heard they are here
  let picking = 0;          // a picker this page opened is up (ms), so a hidden page is not "away"
  const enc = (b) => W.b64u.enc(b);
  const dec = (s) => W.b64u.dec(s);
  const send = (o) => !!(W.relaySendMsg && W.relaySendMsg(o));
  const say = (extra) =>
    emit(Object.assign({
      type: "passkey", state: P.state, busy: P.busy, need: P.need, note: P.note,
      supported: P.supported, platform: P.platform, origin: location.origin,
    }, extra || {}));

  // The daemon's two refusals (NO_PASSKEY and LOCKED in crates/server/src/passkey/mod.rs).
  W.passkeyIsLock = (text) => /^This needs a passkey|^Locked — unlock with/.test(String(text || ""));

  if (P.supported && PublicKeyCredential.isUserVerifyingPlatformAuthenticatorAvailable) {
    PublicKeyCredential.isUserVerifyingPlatformAuthenticatorAvailable()
      .then((yes) => { P.platform = !!yes; say(); })
      .catch(() => {});
  }

  function ask(what, fn) {
    P.need = what;
    then = fn || null;
    P.note = "";
    send({ type: "passkey_get" });
    say();
  }

  W.passkeyThen = (what, fn) => {
    if (P.state.unlocked) {
      lastAct = Date.now();
      if (fn) fn(); else say({ go: what });
      return true;
    }
    ask(what, fn);
    return false;
  };

  // Busy never outlasts the computer's silence: waiting on its answer (not on the person's own
  // passkey prompt, which has its own timeout) gives up after WAIT and says so, back to a button
  // that works. A button that just stays "waiting" is how this looked broken (2026-10-30).
  const WAIT = 20000;
  const OFFLINE = "Not connected to the computer right now — try again when it is back.";
  let waitTimer = 0;
  const waitFor = () => {
    clearTimeout(waitTimer);
    waitTimer = setTimeout(() => {
      if (!P.busy) return;
      P.busy = false;
      P.note = "The computer didn't answer — check that it is online, then try again.";
      if (W.rlog) W.rlog("passkey: no answer from the computer in " + WAIT / 1000 + " s");
      say();
    }, WAIT);
  };
  const settle = () => clearTimeout(waitTimer);
  // Send, and wait for the computer's answer; or say at once that the link is down.
  const ask_ = (msg) => {
    if (send(msg)) { waitFor(); return true; }
    settle();
    P.busy = false;
    P.note = OFFLINE;
    say();
    return false;
  };

  W.passkeyCancel = () => { settle(); P.need = ""; then = null; P.note = ""; P.busy = false; say(); };

  W.passkeyGo = (purpose) => {
    if (P.busy) return;
    P.note = "";
    P.busy = true;
    say();
    purpose = purpose || (P.state.registered ? "unlock" : "register");
    ask_({ type: "passkey_begin", purpose });
  };

  // Add one, or replace this device's (the daemon allows that only while unlocked).
  W.passkeyAdd = () => {
    if (!P.need) P.need = "add";
    P.adding = true;
    W.passkeyGo("register");
  };

  // Straight into the ceremony for `what` — add one if there is none, else unlock — and run
  // `fn` once unlocked. For a door that is already open (the trackpad's refusal card).
  W.passkeyFor = (what, fn) => {
    P.need = what;
    then = fn || null;
    if (!P.state.registered) P.adding = true;
    W.passkeyGo(P.state.registered ? "unlock" : "register");
  };

  W.passkeyLockNow = () => {
    if (P.state.unlocked) send({ type: "passkey_lock" });
    P.state = Object.assign({}, P.state, { unlocked: false, idle_left_s: 0 });
    say();
  };

  // How long an unlock lasts on this device: "15m" | "1h" | "8h" | "lock" (until it locks
  // itself), and whether leaving the app locks it. Only while unlocked — the daemon checks too.
  W.passkeyStay = (stay, lockOnLeave) => {
    if (!P.state.unlocked) return;
    if (typeof lockOnLeave !== "boolean") {
      // "Until I lock it" starts with leaving the app not locking it; the switch can turn it back.
      lockOnLeave = stay === "lock" ? false : P.state.stay === "lock" ? true : P.state.lock_on_leave !== false;
    }
    P.state = Object.assign({}, P.state, { stay, lock_on_leave: lockOnLeave });
    P.note = "";
    say();
    if (!send({ type: "passkey_stay", stay, lock_on_leave: lockOnLeave })) {
      P.note = OFFLINE;
      say();
    }
  };

  function friendly(e) {
    const name = (e && e.name) || "";
    if (name === "NotAllowedError") return "Cancelled, or your passkey didn't answer in time — try again.";
    if (name === "InvalidStateError") return "This phone already holds a passkey for wado. Use it to unlock, or remove the old one on the computer with `wado passkey remove`.";
    if (name === "SecurityError") return "This page's address can't hold a passkey — open wado over https.";
    if (name === "NotSupportedError") return "This browser can't make a passkey with a screen lock. Set a screen lock (PIN, fingerprint or face) on this device, or use Chrome or Safari.";
    return "The passkey didn't work: " + ((e && e.message) || e);
  }

  // The daemon's challenge: make or use the passkey over it.
  W.relayOn("passkey_challenge", async (m) => {
    settle(); // the person's own prompt now: its timeout, not ours
    picking = Date.now();
    P.busy = true;
    say();
    try {
      if (m.purpose === "register") {
        const name = W.deviceName || "this device";
        const c = await navigator.credentials.create({
          publicKey: {
            challenge: dec(m.challenge),
            rp: { name: "wado", id: location.hostname },
            user: { id: dec(m.user_id), name, displayName: name },
            pubKeyCredParams: [{ type: "public-key", alg: -7 }],
            authenticatorSelection: { userVerification: "required", residentKey: "preferred" },
            attestation: "none",
            timeout: 120000,
          },
        });
        ask_({
          type: "passkey_register",
          client_data: enc(c.response.clientDataJSON),
          attestation: enc(c.response.attestationObject),
        });
      } else {
        const a = await navigator.credentials.get({
          publicKey: {
            challenge: dec(m.challenge),
            rpId: location.hostname,
            allowCredentials: m.cred_id ? [{ type: "public-key", id: dec(m.cred_id) }] : [],
            userVerification: "required",
            timeout: 120000,
          },
        });
        ask_({
          type: "passkey_unlock",
          cred_id: enc(a.rawId),
          client_data: enc(a.response.clientDataJSON),
          auth_data: enc(a.response.authenticatorData),
          signature: enc(a.response.signature),
        });
      }
    } catch (e) {
      P.busy = false;
      P.note = friendly(e);
      if (W.rlog) W.rlog("passkey: " + ((e && e.name) || "") + " " + ((e && e.message) || e));
      say();
    }
  });

  W.relayOn("passkey_state", (m) => {
    const was = P.state.unlocked;
    const adding = P.adding;
    P.adding = false;
    P.state = m.state || P.state;
    settle();
    P.busy = false;
    P.note = P.state.error || "";
    let go = "";
    if (P.offer) {
      P.offer = false;
      if (!P.state.registered && !P.need) P.need = "paired";
    }
    if (P.state.unlocked && !was) {
      lastAct = Date.now();
      lastKeep = Date.now();
      // The shells were withheld while locked: their list and scrollback come now.
      if (W.shellsRequest) W.shellsRequest();
      if (P.need) {
        const what = P.need, fn = then;
        P.need = ""; then = null;
        if (fn) { try { fn(); } catch (_) {} } else go = what;
      }
    } else if (adding && m.state && !m.state.error && P.state.registered && P.state.unlocked) {
      // A passkey replaced while already unlocked: done, though nothing unlocked anew.
      go = P.need; P.need = ""; then = null;
    } else if (was && !P.state.unlocked && !P.need) {
      // The computer locked it (idle, or another device took the seat) while it looked open.
      P.need = "again";
    }
    say({ go });
  });

  // A new link: the daemon has locked every window bound to a link, so this page starts locked
  // too — unless this device chose a longer unlock, which outlives a reconnect; the daemon's
  // answer to `passkey_get` settles it either way.
  {
    const up = W._relayHandlers.__up;
    W.relayOn("__up", (m) => {
      if (up) up(m);
      const kept = P.state.unlocked && P.state.stay && P.state.stay !== "15m";
      if (!kept) P.state = Object.assign({}, P.state, { unlocked: false, idle_left_s: 0 });
      // Just paired with a code: offer the passkey as the pairing's last step.
      P.offer = !!W.e2eJustPaired;
      W.e2eJustPaired = false;
      send({ type: "passkey_get" });
    });
  }

  // ── Locking ─────────────────────────────────────────────────────────────────
  // A file chooser this page opened hides it on a phone: not the person leaving.
  document.addEventListener("click", (e) => {
    const t = e.target;
    if (t && t.matches && t.matches("input[type=file]")) picking = Date.now();
  }, true);
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "visible") { picking = 0; return; }
    if (picking && Date.now() - picking < 120000) return;
    // Unless this device turned "Lock when I leave the app" off (an older daemon never says).
    if (P.state.unlocked && P.state.lock_on_leave !== false) W.passkeyLockNow();
  });
  const touched = () => {
    lastAct = Date.now();
    if (P.state.unlocked && lastAct - lastKeep > 60000) {
      lastKeep = lastAct;
      send({ type: "passkey_keepalive" });
    }
  };
  for (const ev of ["pointerdown", "keydown", "touchstart", "wheel"]) {
    document.addEventListener(ev, touched, { capture: true, passive: true });
  }
  setInterval(() => {
    if (!P.state.unlocked) return;
    if (P.state.stay === "lock") return; // until it locks itself: no idle clock
    const idle = (P.state.idle_s || 900) * 1000;
    if (Date.now() - lastAct > idle) {
      if (W.rlog) W.rlog("passkey: idle — locked");
      W.passkeyLockNow();
    }
  }, 15000);

  // ── The doors' refusals show the lock instead of an error ───────────────────
  if (W.files) {
    const req = W.files.req, stream = W.files.stream;
    const seen = (text) => { if (W.passkeyIsLock(text)) { P.state.unlocked = false; ask("files"); } };
    W.files.req = (op, args) => req(op, args).catch((e) => { seen(e && e.message); throw e; });
    W.files.stream = (op, args, h) => stream(op, args, Object.assign({}, h, {
      msg: (m) => { if (m && m.err) seen(m.err); return h.msg(m); },
    }));
  }
  // Every way in to the trackpad (its door, a widget, the app icon's shortcut) goes through this.
  if (W.padOpen) {
    const open = W.padOpen;
    W.padOpen = () => W.passkeyThen("pad", open);
  }
  if (W.pad && W.pad.onMsg) {
    W.pad.onMsg((m) => {
      // The pad stays open: its refusal card (js/pad_why.js) offers the passkey right there.
      // Asked, not assumed: a refusal can arrive late (the direct offer's, after an unlock in
      // between), and the computer's answer is what stands.
      // Shown locked at once all the same, so the answer is not read as "it locked itself while
      // you were away" — the pad's card is already saying so, with its own Unlock.
      if (m && m.k === "err" && W.passkeyIsLock(m.message)) {
        P.state = Object.assign({}, P.state, { unlocked: false, idle_left_s: 0 });
        send({ type: "passkey_get" });
        say();
      }
    });
  }
})();
