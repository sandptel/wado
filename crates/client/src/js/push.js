// wado bridge — Web Push from the selected computer (daemon: crates/server/src/push/), through the
// installed app's service worker (public/sw.js shows the notifications and routes their buttons).
//
//   W.pushOn(kinds)       ask permission, subscribe with the computer's key, send it the subscription
//   W.pushOff()           unsubscribe here and tell the computer
//   W.pushKinds(kinds)    which kinds this device wants from it
//   W.pushTest()          one now, to see it arrive
//
// A browser holds one subscription per service worker, made with one computer's key — so a phone
// gets notifications from one computer at a time; turning them on for another moves them there
// (`wado.pushFrom` remembers which). The computer forgets a subscription the push service says is gone.
//
// To Rust: {type:"push", info, support, permission, standalone, from, busy}.

(() => {
  const send = (o) => !!(W.relaySendMsg && W.relaySendMsg(o));
  const FROM = "wado.pushFrom";
  const P = (W.push = { info: null, busy: false });
  // The Android app (W.native) has no service worker: it subscribes through UnifiedPush instead
  // (js/native_push.js), and shows the notifications itself.
  const native = () => !!(W.native && W.native.pushSubscribe);
  const support = () => native() ||
    !!(navigator.serviceWorker && window.PushManager && window.Notification && window.isSecureContext);
  const standalone = () => native() ||
    !!((window.matchMedia && matchMedia("(display-mode: standalone)").matches) || navigator.standalone);
  const from = () => { try { return localStorage.getItem(FROM) || ""; } catch (_) { return ""; } };
  const here = () => String((W._relayTarget && W._relayTarget.id) || "").replace(/\D/g, "");
  const say = (extra) => emit(Object.assign({
    type: "push", info: P.info, support: support(), standalone: standalone(),
    permission: native() ? W.native.notifications() : window.Notification ? Notification.permission : "unsupported",
    from: from(), here: here(), busy: P.busy,
  }, extra || {}));

  async function reg() {
    if (!support()) throw new Error("This browser can't receive notifications from a website.");
    const r = await navigator.serviceWorker.ready;
    if (!r || !r.pushManager) throw new Error("The app's service worker isn't running — reload wado.");
    return r;
  }

  W.pushOn = async (kinds) => {
    P.busy = true; say();
    try {
      if (!P.info || !P.info.key) throw new Error("The computer hasn't said its notification key yet — try again.");
      if (native()) {
        const j = await W.native.pushSubscribe(P.info.key, here());
        send({
          type: "push_subscribe",
          sub: { endpoint: j.endpoint, p256dh: j.keys.p256dh, auth: j.keys.auth, kind: "unifiedpush" },
          kinds: kinds || (P.info && P.info.kinds) || {},
        });
        return;
      }
      const perm = await Notification.requestPermission();
      if (perm !== "granted") throw new Error(perm === "denied"
        ? "Notifications are blocked for wado in this browser's site settings — allow them there."
        : "Notifications weren't allowed.");
      const r = await reg();
      let sub = await r.pushManager.getSubscription();
      const key = W.b64u.dec(P.info.key);
      const same = sub && sub.options && sub.options.applicationServerKey &&
        W.b64u.enc(sub.options.applicationServerKey) === P.info.key;
      if (sub && !same) { await sub.unsubscribe(); sub = null; }
      if (!sub) sub = await r.pushManager.subscribe({ userVisibleOnly: true, applicationServerKey: key });
      const j = sub.toJSON();
      send({
        type: "push_subscribe",
        sub: { endpoint: j.endpoint, p256dh: j.keys.p256dh, auth: j.keys.auth, kind: "web" },
        kinds: kinds || (P.info && P.info.kinds) || {},
      });
      try { localStorage.setItem(FROM, here()); } catch (_) {}
    } catch (e) {
      P.busy = false;
      say({ error: (e && e.message) || String(e) });
      return;
    }
  };

  W.pushOff = async () => {
    P.busy = true; say();
    send({ type: "push_unsubscribe" });
    if (native()) return;
    try {
      if (from() === here()) {
        const r = await reg();
        const sub = await r.pushManager.getSubscription();
        if (sub) await sub.unsubscribe();
        localStorage.removeItem(FROM);
      }
    } catch (_) {}
  };

  W.pushKinds = (kinds) => send({ type: "push_kinds_set", kinds });
  W.pushTest = () => { P.busy = true; say(); send({ type: "push_test" }); };

  W.relayOn("push_info", (m) => {
    P.info = m.info || null;
    P.busy = false;
    say({ error: (P.info && P.info.error) || "" });
  });
  {
    const up = W._relayHandlers.__up;
    W.relayOn("__up", (m) => { if (up) up(m); send({ type: "push_get" }); });
  }
  // A push while wado is in front comes as a message (sw.js): a toast, not a notification.
  if (navigator.serviceWorker) {
    navigator.serviceWorker.addEventListener("message", (e) => {
      const d = e.data || {};
      if (d.type === "wado-push" && d.note && d.note.kind !== "claude_permission") {
        emit({ type: "captureNote", text: d.note.title + (d.note.body ? " — " + d.note.body : "") });
      }
    });
  }
})();
