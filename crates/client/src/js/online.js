// wado bridge — live status of the saved computers this page is not connected to: does their
// Remote ID have a daemon online at the relay right now (`GET /online/:id`). The selected one
// needs no probe — its relay link says, live. Polled while the landing is up.

let onlineTimer = 0;
W.onlineWatch = (targets) => {
  clearInterval(onlineTimer);
  if (!targets || !targets.length) return;
  const probe = () => targets.forEach(async ([relay, id]) => {
    const key = relay + "|" + id;
    try {
      const r = await fetch(String(relay).replace(/\/$/, "") + "/online/" + encodeURIComponent(id), { cache: "no-store" });
      const j = await r.json();
      emit({ type: "online", key, daemons: j.daemons | 0 });
    } catch (_) {
      emit({ type: "online", key, daemons: -1 }); // relay unreachable (or too old to answer)
    }
  });
  probe();
  onlineTimer = setInterval(probe, 20000);
};
