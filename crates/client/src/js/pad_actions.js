// wado bridge — the things this computer can run for the trackpad (wire: crates/protocol/src/pad.rs;
// daemon: crates/server/src/actions/). Asked for over the pad link when the pad opens:
// `{k:"actions"}` → `{k:"action_list", actions:[{id, name, group, icon}]}`, available ones only.
// Running is `{k:"run", id}` → `{k:"ran", id, ok, message}`.
//
// Both need the `host` grant. Without it the daemon answers an empty list and then a non-fatal
// `err` carrying the grant copy; that err is this module's (`owns`), shown where the actions
// would be rather than as an alert, and the pad keeps working.
//
//   W.padActions.refresh()            ask again
//   W.padActions.list                 [{id, name, group, icon}], in the daemon's order
//   W.padActions.byId(id)
//   W.padActions.denied               the grant copy when this device may not run actions, else ""
//   W.padActions.run(id) → Promise<{ok, message}>   settles on its `ran`, or after 8 s
//   W.padActions.onChange(fn)         the list or `denied` changed
//   W.padActions.owns(msg)            the `err` was the list's refusal

(() => {
  const A = (W.padActions = { list: [], denied: "" });
  const NO_ANSWER_MS = 8000;
  const changeFns = [];
  const waiting = new Map(); // id → [resolve…], oldest first
  const owned = new WeakSet();
  let expectRefusal = false;

  A.onChange = (fn) => changeFns.push(fn);
  const emit = () => { for (const fn of changeFns) { try { fn(A); } catch (_) {} } };
  A.byId = (id) => A.list.find((a) => a.id === id) || null;
  A.owns = (m) => owned.has(m);

  A.refresh = () => W.pad.send({ k: "actions" }, false);

  function settle(id, res) {
    const q = waiting.get(id);
    if (!q || !q.length) return;
    const done = q.shift();
    if (!q.length) waiting.delete(id);
    done(res);
  }

  A.run = (id) => new Promise((ok) => {
    const q = waiting.get(id) || [];
    let done = false;
    const once = (res) => { if (!done) { done = true; ok(res); } };
    const give = (message) => {
      const i = q.indexOf(once);
      if (i < 0) return;
      q.splice(i, 1);
      if (!q.length && waiting.get(id) === q) waiting.delete(id);
      once({ ok: false, message });
    };
    q.push(once);
    waiting.set(id, q);
    if (!W.pad.send({ k: "run", id }, false)) return give("Not connected to the computer.");
    setTimeout(() => give("The computer didn't answer."), NO_ANSWER_MS);
  });

  W.pad.onMsg((m) => {
    if (m.k === "action_list") {
      A.list = Array.isArray(m.actions) ? m.actions.filter((a) => a && a.id) : [];
      expectRefusal = !A.list.length;
      if (A.list.length) A.denied = "";
      emit();
    } else if (m.k === "ran") {
      settle(m.id, { ok: !!m.ok, message: m.message || "" });
    } else if (m.k === "err" && !m.fatal && expectRefusal) {
      expectRefusal = false;
      owned.add(m);
      A.denied = m.message || "";
      emit();
    } else if (m.k !== "pong") {
      expectRefusal = false;
    }
  });
})();
