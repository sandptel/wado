// wado bridge — the viewer's archives: what a zip holds (and the formats that are zips: jar, apk,
// epub, docx, xlsx…), listed by the computer from the archive's index — names and sizes, never
// the contents, so it is cheap whatever the size and fine on the relay's limited lane
// (server::files::entries). A tar or a 7z has no index to read; for those, an honest card.

(() => {
  const F = W.files;
  const V = F.viewer;
  const h = F.h;
  const ZIPS = new Set("zip jar apk epub docx xlsx pptx odt ods odp".split(" "));

  V.add("archive", {
    fetch: "none",
    render(ctx) {
      const { e, stage } = ctx;
      const x = V.ext(e.name);
      if (!ZIPS.has(x)) {
        stage.replaceChildren(V.cantPreview(e, `What is inside a .${x} can only be seen by reading all of it. Download it, or open it on the computer.`));
        return null;
      }
      stage.replaceChildren(h("div", { class: "fspin", role: "status", "aria-label": "Loading" }));
      // An older wado drops a request it does not know without an answer: give it 8 s.
      const late = new Promise((_, no) => setTimeout(() => no(new Error("no answer")), 8000));
      Promise.race([F.req("entries", { path: e.path }), late]).then((r) => {
        if (!ctx.alive()) return;
        const files = r.entries.filter((x) => !x.dir);
        const folders = r.entries.length - files.length;
        const unpacked = files.reduce((a, x) => a + (x.size || 0), 0);
        const rows = r.entries.map((x) => {
          const depth = x.name.replace(/\/$/, "").split("/").length - 1;
          const leaf = x.name.replace(/\/$/, "").split("/").pop();
          return h("li", { class: "fvarow" + (x.dir ? " dir" : ""), style: `--d:${Math.min(depth, 6)}` },
            V.ico(x.dir ? "folder" : F.kind(leaf).icon), h("span", { class: "fvaname", title: x.name }, leaf),
            x.dir ? null : h("small", {}, F.fmtSize(x.size)));
        });
        const note = r.more ? h("p", { class: "fvnote" }, `Showing the first ${r.entries.length.toLocaleString()} of ${r.total.toLocaleString()} entries.`) : null;
        stage.replaceChildren(h("div", { class: "fvscroll" }, note,
          h("ul", { class: "fvalist", "aria-label": "Inside " + e.name }, ...rows)));
        const summary = `${files.length.toLocaleString()}${r.more ? "+" : ""} files${folders ? ` · ${folders} folders` : ""}`;
        ctx.tools(h("span", { class: "fvcount" }, summary));
        ctx.add("Inside", [["Entries", r.total.toLocaleString()], ["Unpacked", (r.more ? "over " : "") + F.fmtSize(unpacked)]]);
      }, (err) => {
        if (!ctx.alive()) return;
        const old = /no answer/.test(String(err.message || err));
        ctx.fail(old ? "The computer's wado is older and cannot list archives yet. Download it, or open it on the computer."
          : `Its contents could not be listed: ${err.message || err}.`);
      });
      return null;
    },
  });
})();
