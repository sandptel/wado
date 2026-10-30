// wado bridge — the viewer's fonts (ttf, otf, woff, woff2): the font loaded from its bytes as a
// FontFace in this page only, and set in a sample — a pangram at a few sizes, the alphabet and
// the figures. A font the browser refuses gets the honest card.

(() => {
  const F = W.files;
  const V = F.viewer;
  const h = F.h;
  let n = 0;

  V.add("font", {
    fetch: "whole",
    render(ctx) {
      const { e, stage } = ctx;
      const family = "wado-viewer-font-" + ++n;
      let face = null;
      ctx.bytes.blob.arrayBuffer().then((buf) => new FontFace(family, buf).load()).then((f) => {
        if (!ctx.alive()) return;
        face = f;
        document.fonts.add(f);
        const name = e.name.replace(/\.[^.]+$/, "").replace(/[-_]+/g, " ");
        const s = (cls, text) => h("p", { class: cls, style: `font-family:"${family}"` }, text);
        stage.replaceChildren(h("div", { class: "fvscroll" }, h("div", { class: "fvfont" },
          s("fvf1", name),
          s("fvf2", "The quick brown fox jumps over the lazy dog."),
          s("fvf3", "ABCDEFGHIJKLMNOPQRSTUVWXYZ"),
          s("fvf3", "abcdefghijklmnopqrstuvwxyz"),
          s("fvf3", "0123456789 & ! ? @ # % ( ) “ ” → €"),
          s("fvf4", "Grumpy wizards make toxic brew for the evil queen and jack. Sphinx of black quartz, judge my vow."))));
        ctx.add("Font", [["Family", f.family === family ? name : f.family], ["Style", [f.weight, f.style].filter((x) => x && x !== "normal").join(" ") || "Regular"]]);
      }).catch(() => ctx.fail("This browser could not read this font."));
      return { destroy() { if (face) document.fonts.delete(face); } };
    },
  });
})();
