// wado bridge — the viewer's PDFs: the browser's own PDF viewer, in a frame, on a desktop. Phone
// browsers hand PDFs to an app instead of drawing them in a page, so there the card says so and
// opens it in one. No PDF library: the browser already has the best renderer it can.
//
// The frame is the browser's built-in viewer (a plugin, never this page's script), so a PDF's own
// script is the viewer's business, sandboxed by the browser as for any PDF on the web.

(() => {
  const F = W.files;
  const V = F.viewer;
  const h = F.h;

  // Desktop browsers draw PDFs in a frame; phone browsers (and some tablets) do not.
  const inline = () => navigator.pdfViewerEnabled !== false && !matchMedia("(pointer: coarse)").matches;

  V.add("pdf", {
    fetch: "whole",
    render(ctx) {
      const { e, bytes: v, stage } = ctx;
      if (inline()) {
        stage.replaceChildren(h("iframe", { class: "fvpdf", src: v.url, title: e.name }));
        return null;
      }
      const open = h("a", { class: "fvaction primary", href: v.url, target: "_blank", rel: "noopener", download: e.name },
        V.ico("pdf"), h("span", {}, "Open the PDF"));
      const card = V.card({
        icon: "pdf",
        title: "Opens in your PDF app",
        text: "This browser hands PDFs to an app rather than showing them in a page. It is here on this phone now.",
        rows: V.facts(e),
      });
      card.querySelector(".fvcardx").append(h("div", { class: "fvcardacts" }, open));
      stage.replaceChildren(card);
      return null;
    },
  });
})();
