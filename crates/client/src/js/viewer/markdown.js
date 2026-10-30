// wado bridge — the viewer's Markdown: read as plain formatted text — headings, paragraphs,
// lists, quotes, code blocks, rules; bold, italic, code and links inline — with a toggle to the
// source in the code view. Built as DOM text node by node: no markup from the file ever becomes
// markup here (no HTML passes through, images are named, not loaded), and only http(s) links
// become links.

(() => {
  const F = W.files;
  const V = F.viewer;
  const h = F.h;

  // Inline: **bold**, *italic*, `code`, [text](url). Everything else is text — snake_case
  // included, which is why `_` never means italic here.
  const INLINE = /(\*\*|__)(.+?)\1|(\*)(?!\s)(.+?)\3|`([^`]+)`|!?\[([^\]]*)\]\(([^)\s]+)[^)]*\)/g;
  function inline(text) {
    const out = [];
    let i = 0;
    // A regex of its own per call: this recurses, and a shared `lastIndex` would be clobbered.
    const re = new RegExp(INLINE.source, "g");
    for (let m; (m = re.exec(text)); ) {
      if (m.index > i) out.push(text.slice(i, m.index));
      if (m[2] !== undefined) out.push(h("strong", {}, ...inline(m[2])));
      else if (m[4] !== undefined) out.push(h("em", {}, ...inline(m[4])));
      else if (m[5] !== undefined) out.push(h("code", {}, m[5]));
      else if (m[0][0] === "!") out.push(h("span", { class: "fvmdimg" }, "[picture: " + (m[6] || m[7]) + "]"));
      else if (/^https?:\/\//i.test(m[7])) out.push(h("a", { href: m[7], target: "_blank", rel: "noopener noreferrer" }, ...inline(m[6] || m[7])));
      else out.push(h("span", { class: "fvmdlink" }, ...inline(m[6] || m[7])));
      i = m.index + m[0].length;
    }
    if (i < text.length) out.push(text.slice(i));
    return out;
  }

  function render(src) {
    const root = h("article", { class: "fvmd" });
    const lines = src.replace(/\r\n?/g, "\n").split("\n");
    let para = [], list = null;
    const flush = () => {
      if (para.length) root.append(h("p", {}, ...inline(para.join(" "))));
      para = [];
    };
    const endList = () => { list = null; };
    for (let i = 0; i < lines.length; i++) {
      const l = lines[i];
      let m;
      if ((m = /^\s*(```|~~~)/.exec(l))) {
        flush(); endList();
        const body = [];
        while (++i < lines.length && !lines[i].trim().startsWith(m[1])) body.push(lines[i]);
        root.append(h("pre", {}, h("code", {}, body.join("\n"))));
      } else if ((m = /^(#{1,6})\s+(.*?)\s*#*\s*$/.exec(l))) {
        flush(); endList();
        root.append(h("h" + Math.min(6, m[1].length + 1), {}, ...inline(m[2])));
      } else if (/^\s*([-*_])(\s*\1){2,}\s*$/.test(l)) {
        flush(); endList();
        root.append(h("hr"));
      } else if ((m = /^\s*>\s?(.*)$/.exec(l))) {
        flush(); endList();
        const q = [m[1]];
        while (i + 1 < lines.length && /^\s*>/.test(lines[i + 1])) q.push(lines[++i].replace(/^\s*>\s?/, ""));
        root.append(h("blockquote", {}, ...inline(q.join(" "))));
      } else if ((m = /^\s*([-*+]|\d+[.)])\s+(\[[ xX]\]\s+)?(.*)$/.exec(l))) {
        flush();
        const ordered = /\d/.test(m[1]);
        if (!list || list.ordered !== ordered) { list = { ordered, el: h(ordered ? "ol" : "ul") }; root.append(list.el); }
        const box = m[2] ? h("span", { class: "fvmdbox", "aria-label": /x/i.test(m[2]) ? "done" : "to do" }, /x/i.test(m[2]) ? "☑ " : "☐ ") : null;
        list.el.append(h("li", {}, box, ...inline(m[3])));
      } else if (!l.trim()) {
        flush(); endList();
      } else {
        endList();
        para.push(l.trim());
      }
    }
    flush();
    return root;
  }

  V.add("markdown", {
    fetch: "head",
    render(ctx) {
      const { stage } = ctx;
      ctx.bytes.blob.text().then((t) => {
        if (!ctx.alive()) return;
        let source = false;
        const formatted = () => {
          stage.replaceChildren(h("div", { class: "fvscroll" },
            ctx.bytes.cut ? h("p", { class: "fvnote" }, `Showing the first ${F.fmtSize(V.TEXT_MAX)} — download it for the rest.`) : null,
            render(t)));
          ctx.tools(toggle);
        };
        const toggle = h("button", { class: "fvchip", "aria-pressed": "false", onclick: () => {
          source = !source;
          toggle.setAttribute("aria-pressed", String(source));
          // The source in the code view, with this toggle beside its Wrap.
          if (source) V.codeView({ ...ctx, tools: (...k) => ctx.tools(toggle, ...k), add: () => {} }, t);
          else formatted();
        } }, V.ico("code"), h("span", {}, "Source"));
        formatted();
      });
      return null;
    },
  });
})();
