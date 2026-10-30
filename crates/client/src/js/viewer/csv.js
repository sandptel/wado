// wado bridge — the viewer's CSV and TSV: a table, the first row as its header, sticky; up to
// ROWS rows drawn, the rest counted. Quoted fields, doubled quotes and line breaks inside quotes
// are read as a spreadsheet would. Cells are text, never markup.

(() => {
  const F = W.files;
  const V = F.viewer;
  const h = F.h;
  const ROWS = 500;

  // The rows of `text`, at most `max` + 1 (to know there are more).
  function parse(text, sep, max) {
    const rows = [];
    let row = [], cell = "", q = false;
    for (let i = 0; i < text.length && rows.length <= max; i++) {
      const c = text[i];
      if (q) {
        if (c === '"' && text[i + 1] === '"') { cell += '"'; i++; }
        else if (c === '"') q = false;
        else cell += c;
      } else if (c === '"' && cell === "") q = true;
      else if (c === sep) { row.push(cell); cell = ""; }
      else if (c === "\n" || c === "\r") {
        if (c === "\r" && text[i + 1] === "\n") i++;
        row.push(cell); rows.push(row); row = []; cell = "";
      } else cell += c;
    }
    if (cell !== "" || row.length) { row.push(cell); rows.push(row); }
    return rows;
  }

  V.add("csv", {
    fetch: "head",
    render(ctx) {
      const { e, stage } = ctx;
      ctx.bytes.blob.text().then((t) => {
        if (!ctx.alive()) return;
        const sep = V.ext(e.name) === "tsv" ? "\t" : t.slice(0, 2000).split(";").length > t.slice(0, 2000).split(",").length ? ";" : ",";
        const rows = parse(t, sep, ROWS + 1);
        if (!rows.length) return ctx.fail("This table is empty.");
        const head = rows[0], body = rows.slice(1, ROWS + 1);
        const total = t.split("\n").filter(Boolean).length - 1;
        const more = rows.length > ROWS + 1 || ctx.bytes.cut;
        const table = h("table", { class: "fvtable" },
          h("thead", {}, h("tr", {}, h("th", { scope: "col", class: "fvrn" }, "#"), ...head.map((c) => h("th", { scope: "col" }, c)))),
          h("tbody", {}, ...body.map((r, i) => h("tr", {}, h("td", { class: "fvrn" }, String(i + 1)), ...head.map((_, j) => h("td", {}, r[j] ?? ""))))));
        const note = more ? `Showing the first ${body.length.toLocaleString()} rows${ctx.bytes.cut ? "" : " of " + total.toLocaleString()} — download it for the rest.` : "";
        stage.replaceChildren(h("div", { class: "fvscroll fvscrollx", tabindex: "0", role: "region", "aria-label": e.name },
          note ? h("p", { class: "fvnote" }, note) : null, table));
        ctx.tools(h("span", { class: "fvcount" }, `${head.length} columns · ${(ctx.bytes.cut ? body.length + "+" : total.toLocaleString())} rows`));
        ctx.add("Table", [["Columns", String(head.length)], ["Rows", ctx.bytes.cut ? body.length + "+" : total.toLocaleString()]]);
      });
      return null;
    },
  });
})();
