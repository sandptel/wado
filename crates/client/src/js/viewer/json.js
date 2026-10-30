// wado bridge — the viewer's JSON: pretty-printed, two spaces, in the code view (line numbers,
// colour, wrap). JSON lines are pretty-printed one record at a time. What does not parse — or was
// cut at the size cap — is shown as it is, and the note says why.

(() => {
  const F = W.files;
  const V = F.viewer;

  function pretty(text, lines) {
    if (!lines) return JSON.stringify(JSON.parse(text), null, 2);
    return text.split(/\r?\n/).filter((l) => l.trim()).map((l) => JSON.stringify(JSON.parse(l), null, 2)).join("\n");
  }

  V.add("json", {
    fetch: "head",
    render(ctx) {
      ctx.bytes.blob.text().then((t) => {
        if (!ctx.alive()) return;
        let body = t, note = "";
        try { body = pretty(t, V.ext(ctx.e.name) === "jsonl"); }
        catch (err) {
          note = ctx.bytes.cut
            ? `Showing the first ${F.fmtSize(V.TEXT_MAX)} as it is — the whole file is needed to lay it out.`
            : `Shown as it is: it is not valid JSON (${String(err.message || err).replace(/^JSON\.parse: /, "")}).`;
        }
        V.codeView(ctx, body, { note });
      });
      return null;
    },
  });
})();
