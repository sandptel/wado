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
//   W.passkeyRequire(on)      ask this phone for its passkey, or stop (off only while unlocked)
//   (approve on the computer: js/passkey_approve.js; the words for a failure: passkey_errors.js)
//
// Never stuck (2026-10-30): each stage has a bound and an exit. Waiting on the computer gives up
// after WAIT; waiting on the phone's own prompt after AUTH_WAIT, by aborting it — WebAuthn's
// `timeout` is advisory, and Android's prompt can fail to show without ever answering. One
// ceremony at a time: a new challenge aborts the one before (an orphaned prompt blocked every
// later one), and Not now aborts it too.
//
// It locks itself after the daemon's idle window without the person touching the app, and the
// moment the app goes to the background — except while a picker this page opened is up (the
// passkey prompt itself, a file chooser), which hides the page without anyone leaving it.
//
// To Rust: {type:"passkey", state, busy, stage, kind, need, note, approve, supported, platform,
// go}. `need` set means "show the sheet for this door"; `go` names a door to open now that it is
// unlocked; `stage` is what busy waits on ("computer" | "prompt"); `kind` what a failure calls
// for (passkey_errors.js); `approve` how an approval on the computer stands.

(() => {
  const P = (W.passkey = {
    state: { registered: false, unlocked: false, idle_left_s: 0, idle_s: 900, error: "", origin: "" },
    busy: false,
    stage: "",
    kind: "",
    need: "",
    note: "",
    approve: null,
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
      type: "passkey", state: P.state, busy: P.busy, stage: P.busy ? P.stage : "", kind: P.kind,
      need: P.need, note: P.note, approve: P.approve,
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
    P.kind = "";
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
  // The phone's own prompt: a person needs a few seconds, a PIN fallback some more — not a minute.
  // (`W.passkeyAuthWaitMs` shortens it for the e2e script.)
  const AUTH_WAIT = 60000;
  const authWait = () => W.passkeyAuthWaitMs || AUTH_WAIT;
  const OFFLINE = "Not connected to the computer right now — try again when it is back.";
  let waitTimer = 0;
  const waitFor = () => {
    clearTimeout(waitTimer);
    P.stage = "computer";
    waitTimer = setTimeout(() => {
      if (!P.busy || cer) return;
      P.busy = false;
      P.kind = "retry";
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
    P.kind = "retry";
    P.note = OFFLINE;
    say();
    return false;
  };

  // ── One ceremony at a time ──────────────────────────────────────────────────
  // `cer` is the phone's prompt now up: its AbortController and its deadline. A newer one, Not
  // now, or the deadline ends it — the promise settles even if the platform never answers.
  let cer = null;
  let seq = 0;
  let wanted = 0; // when this page last asked for a challenge: a challenge nobody asked for is not run
  const stop = (why) => { if (cer) cer.end(why); };
  const ceremony = (run) => new Promise((ok, no) => {
    stop("replaced");
    const me = { n: ++seq, ac: new AbortController() };
    let over = false;
    me.end = (why) => {
      if (over) return;
      over = true;
      clearTimeout(me.timer);
      if (cer === me) cer = null;
      try { me.ac.abort(why); } catch (_) {}
      const e = new DOMException("The passkey prompt was " + why + ".", "AbortError");
      e.wadoWhy = why;
      no(e);
    };
    me.timer = setTimeout(() => {
      if (W.rlog) W.rlog("passkey: the prompt didn't answer in " + authWait() / 1000 + " s — given up");
      me.end("timeout");
    }, authWait());
    cer = me;
    let p;
    try { p = run(me.ac.signal); } catch (e) { p = Promise.reject(e); }
    Promise.resolve(p).then((v) => {
      if (over) return;
      over = true; clearTimeout(me.timer); if (cer === me) cer = null; ok(v);
    }, (e) => {
      if (over) return;
      over = true; clearTimeout(me.timer); if (cer === me) cer = null; no(e);
    });
  });
  P._stop = stop;
  P._say = say;
  P._settle = settle;

  W.passkeyCancel = () => {
    settle();
    stop("cancelled");
    wanted = 0;
    if (P.approve && P.approve.state === "waiting") send({ type: "passkey_approve_cancel" });
    P.approve = null;
    P.need = ""; then = null; P.note = ""; P.kind = ""; P.busy = false;
    say();
  };

  W.passkeyGo = (purpose) => {
    if (P.busy) return;
    P.note = "";
    P.kind = "";
    P.busy = true;
    wanted = Date.now();
    say();
    purpose = purpose || (P.state.registered ? "unlock" : "register");
    ask_({ type: "passkey_begin", purpose });
  };

  // Ask this phone for its passkey, or stop asking. Off only while unlocked (the daemon checks).
  W.passkeyRequire = (on) => {
    P.note = "";
    if (!send({ type: "passkey_require", on: !!on })) { P.note = OFFLINE; }
    say();
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

  // The daemon's challenge: make or use the passkey over it.
  W.relayOn("passkey_challenge", async (m) => {
    // Nobody here asked (Not now came first, or it is very late): the prompt stays down. A
    // challenge while one is up is the newest the computer holds — it replaces the old prompt.
    if (!cer && (!wanted || Date.now() - wanted > WAIT + 5000)) {
      if (W.rlog) W.rlog("passkey: a challenge nobody is waiting for — not shown");
      return;
    }
    wanted = 0;
    settle(); // the person's own prompt now: its deadline, not the computer's
    picking = Date.now();
    P.busy = true;
    P.stage = "prompt";
    P.note = "";
    P.kind = "";
    say();
    try {
      if (m.purpose === "register") {
        const name = W.deviceName || "this device";
        const c = await ceremony((signal) => navigator.credentials.create({
          signal,
          publicKey: {
            challenge: dec(m.challenge),
            rp: { name: "wado", id: location.hostname },
            user: { id: dec(m.user_id), name, displayName: name },
            pubKeyCredParams: [{ type: "public-key", alg: -7 }],
            authenticatorSelection: { userVerification: "required", residentKey: "preferred" },
            attestation: "none",
            timeout: authWait(),
          },
        }));
        ask_({
          type: "passkey_register",
          client_data: enc(c.response.clientDataJSON),
          attestation: enc(c.response.attestationObject),
        });
      } else {
        const a = await ceremony((signal) => navigator.credentials.get({
          signal,
          publicKey: {
            challenge: dec(m.challenge),
            rpId: location.hostname,
            allowCredentials: m.cred_id ? [{ type: "public-key", id: dec(m.cred_id) }] : [],
            userVerification: "required",
            timeout: authWait(),
          },
        }));
        ask_({
          type: "passkey_unlock",
          cred_id: enc(a.rawId),
          client_data: enc(a.response.clientDataJSON),
          auth_data: enc(a.response.authenticatorData),
          signature: enc(a.response.signature),
        });
      }
    } catch (e) {
      const why = e && e.wadoWhy;
      if (W.rlog) W.rlog("passkey: " + ((e && e.name) || "") + " " + (why || (e && e.wadoCode) || "") + " " + ((e && e.message) || e));
      // Replaced by a newer prompt, or Not now: that one, or the sheet, says what stands.
      if (why === "replaced" || why === "cancelled") return;
      const w = W.passkeyWhy(e, { purpose: m.purpose, timedOut: why === "timeout" });
      P.busy = false;
      P.kind = w.kind;
      P.note = w.note;
      say();
    }
  });

  W.relayOn("passkey_state", (m) => {
    const was = P.state.unlocked;
    const adding = P.adding;
    P.adding = false;
    P.state = m.state || P.state;
    settle();
    // A prompt still up (a reconnect asked where it stands): it is still the person's turn.
    P.busy = !!cer;
    P.note = P.state.error || "";
    P.kind = P.note ? "retry" : "";
    if (P.state.unlocked) P.approve = null;
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
    if (P.state.unlocked && P.state.required !== false && P.state.lock_on_leave !== false) W.passkeyLockNow();
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
    if (P.state.required === false) return; // not asked for a passkey: nothing to lock
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
