// wado bridge — the viewer's card for everything with no preview: an executable, a disk image,
// a format nothing here reads. It says so plainly, states what is known — type, size, modified —
// and offers what can be done: download it, or open it on the computer. Nothing is fetched.

(() => {
  const V = W.files.viewer;
  V.add("info", {
    fetch: "none",
    render(ctx) {
      ctx.stage.replaceChildren(V.cantPreview(ctx.e));
      return null;
    },
  });
})();
