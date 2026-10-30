// wado bridge — the viewer's honest cards: what to show instead of a preview, and why.
//
//   V.card({ icon, title, text, rows, actions })   a centred card; `actions`: [{ label, icon, run, primary }]
//   V.cantPreview(entry, why)                       "Can't preview .xyz here" — Download / Open on the computer
//   V.waitDirect(entry)                             the relay's limited lane: bigger previews wait for Direct
//   V.askLoad(entry, load)                          a very big file: load it, or download instead
//   V.facts(entry) → rows                           Type, Size, Modified — the facts every card can state

(() => {
  const F = W.files;
  const V = F.viewer;
  const h = F.h;
  const ico = (n) => h("span", { class: "fi", "aria-hidden": "true", html: F.icon(n) });

  V.ico = ico;
  V.when = (s) => (s ? new Date(s * 1000).toLocaleString([], { dateStyle: "medium", timeStyle: "short" }) : "");
  V.facts = (e) => [
    ["Type", V.type(e.name)],
    ["Size", F.fmtSize(e.size)],
    ["Modified", V.when(e.mtime)],
  ].filter(([, v]) => v);

  V.card = ({ icon = "file", title, text, rows = [], actions = [] }) => {
    const card = h("div", { class: "fvcardx", role: "group", "aria-label": title },
      h("div", { class: "fvcardicon" }, ico(icon)),
      h("b", { class: "fvcardtitle" }, title),
      text ? h("p", {}, text) : null,
      rows.length ? h("dl", { class: "fvfacts" }, ...rows.flatMap(([k, v]) => [h("dt", {}, k), h("dd", {}, v)])) : null,
      actions.length ? h("div", { class: "fvcardacts" }, ...actions.map((a) =>
        h("button", { class: "fvaction" + (a.primary ? " primary" : ""), onclick: a.run }, a.icon ? ico(a.icon) : null, h("span", {}, a.label)))) : null);
    return h("div", { class: "fvcardwrap" }, card);
  };

  const download = (e) => ({ label: "Download", icon: "download", primary: true, run: () => { F.download(e.path, e.name, false); F.toast("Downloading " + e.name); } });
  const openThere = (e) => (F.act && F.act.canOpenThere && F.act.canOpenThere()
    ? { label: "Open on the computer", icon: "monitor", run: () => F.act.openThere(e.path) } : null);
  V.actions = (e) => [download(e), openThere(e)].filter(Boolean);

  V.cantPreview = (e, why) => {
    const x = V.ext(e.name);
    return V.card({
      icon: F.kind(e.name).icon,
      title: x ? `Can't preview .${x} here` : "Can't preview this file here",
      text: why || "There is no preview for this kind of file. Download it, or open it on the computer.",
      rows: V.facts(e),
      actions: V.actions(e),
    });
  };

  V.waitDirect = (e) => V.card({
    icon: "clock",
    title: "This preview waits for a direct connection",
    text: `Right now the computer is reached through wado's relay, which only carries files up to ${F.fmtSize(V.LANE_MAX)}. `
      + `This one is ${F.fmtSize(e.size)}. It opens here as soon as there is a direct path again.`,
    rows: V.facts(e),
    actions: [openThere(e)].filter(Boolean),
  });

  V.askLoad = (e, load) => V.card({
    icon: F.kind(e.name).icon,
    title: "A big file",
    text: `${F.fmtSize(e.size)} — it has to come over in full before it shows here.`,
    actions: [{ label: "Load it", icon: "download", primary: true, run: load }, { ...download(e), primary: false, label: "Download instead" }],
  });
})();
