// wado bridge — notifications and push in the Android app (native.js; nothing without it).
//
//   page → app  notify   {kind, id, title, body, actions?: [{id, label}]}
//                        kinds: "app" (a session app's notification), "claude_permission",
//                        "claude_done", "computer_online", "computer_offline", "transfer_done".
//                        The app shows it as a system notification while it is not in front.
//   page → app  notifyClear {kind, id}
//   app → page  notifyAction {kind, id, action, session?, ask?, computer?}   a notification's button
//   page → app  pushSubscribe {vapid, rid}        → app → page  pushEndpoint {endpoint, keys:
//                                                   {p256dh, auth}} | {error}
//   (js/push.js uses it when W.native is up, registering the endpoint as kind "unifiedpush")
//
// Push needs no Google: the app registers with the phone's UnifiedPush distributor (ntfy, …)
// and hands back an endpoint shaped like a Web Push subscription, so the computer's Web Push
// sender (RFC 8291 + VAPID) needs nothing new to reach it. `W.native.pushSubscribe(vapid)` is
// the app's `pushManager.subscribe`; the code that registers a subscription with the computer
// calls whichever exists.
//
// `W.nativeActions[kind] = (id, action, msg) => …` is where a feature answers its notification's
// buttons; a Claude permission prompt's ("allow" / "deny") is wired below.

(() => {
  const N = W.native;
  if (!N) return;
  W.nativeActions = W.nativeActions || {};

  W.native.notify = (kind, id, title, body, actions) =>
    N.send("notify", { kind, id: String(id), title: String(title || ""), body: String(body || ""), actions: actions || [] });
  W.native.notifyClear = (kind, id) => N.send("notifyClear", { kind, id: String(id) });
  N.on("notifyAction", (m) => {
    const fn = W.nativeActions[m.kind];
    if (fn) fn(m.id, m.action, m);
  });

  // A session app's notification (server::notify).
  const prevNote = W._relayHandlers.notification;
  W.relayOn("notification", (msg) => {
    if (prevNote) prevNote(msg);
    W.native.notify("app", msg.id || 0, msg.app ? msg.app + (msg.summary ? " — " + msg.summary : "") : msg.summary, msg.body);
  });
  const prevClosed = W._relayHandlers.notification_closed;
  W.relayOn("notification_closed", (msg) => {
    if (prevClosed) prevClosed(msg);
    W.native.notifyClear("app", msg.id || 0);
  });

  // One UnifiedPush registration per computer (`rid`, its Remote ID), each with its own key —
  // so, unlike a browser's single subscription, every computer can reach this phone.
  let pushWait = null;
  W.native.pushSubscribe = (vapid, rid) => new Promise((ok, no) => {
    pushWait = { ok, no };
    N.send("pushSubscribe", { vapid: String(vapid || ""), rid: String(rid || "") });
  });
  W.native.notifications = () => (window.wadoNativeInfo && window.wadoNativeInfo.notifications) || "default";
  // A Claude permission prompt's Allow / Deny, from a pushed notification (the computer's own
  // PushNote): handed over as the web app's service worker hands it (js/claude.js) — Allow
  // behind the passkey, once that computer's card shows.
  W.nativeActions.claude_permission = (id, action, m) => {
    const rid = String((m && m.computer) || "").replace(/[^0-9]/g, "");
    if (rid) {
      const s = W.loadSettings();
      const p = (Array.isArray(s.profiles) ? s.profiles : [])
        .find((x) => String(x.remote_id || "").replace(/[^0-9]/g, "") === rid);
      if (p) emit({ type: "nativeGo", key: (p.relay_url || "") + "|" + (p.remote_id || ""), go: "" });
    }
    if (W.claudePending) W.claudePending({ session: m && m.session, ask: m && m.ask, act: action });
  };
  N.on("pushEndpoint", (m) => {
    const w = pushWait;
    pushWait = null;
    if (!w) return;
    if (m.error) w.no(new Error(m.error));
    else w.ok({ endpoint: m.endpoint, keys: m.keys || {} });
  });
})();
