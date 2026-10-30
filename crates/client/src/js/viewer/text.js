// wado bridge — the viewer's text and code: monospace with line numbers, a light touch of colour
// (comments, strings, numbers — no grammar), a wrap toggle, and a cap with "Show more".
//
//   V.codeView(ctx, text, { note })   shared with js/viewer/json.js and the Markdown source view
//
// Lines are DOM text, never markup: an .html file is shown as its source and never runs. The file
// is read up to V.TEXT_MAX (2 MB); past that the rest is cut, and a note says so.

(() => {
  const F = W.files;
  const V = F.viewer;
  const h = F.h;
  const STEP = 1500; // lines drawn at once; "Show more" adds as many again
  const WRAP = "wado.viewer.wrap";
  const wrapOn = () => { try { return localStorage.getItem(WRAP) !== "0"; } catch (_) { return true; } };

  // Comments by the families that use them; strings and numbers everywhere.
  const HASH = new Set("py sh bash zsh fish rb yaml yml toml nix conf ini env r pl dockerfile makefile gitignore".split(" "));
  const DASH = new Set("sql lua hs".split(" "));
  const tokenRe = (x) => new RegExp([
    HASH.has(x) ? "#.*$" : DASH.has(x) ? "--.*$" : "\\/\\/.*$|\\/\\*.*?(?:\\*\\/|$)",
    "\"(?:[^\"\\\\]|\\\\.)*\"?|'(?:[^'\\\\]|\\\\.)*'?|`[^`]*`?",
    "\\b\\d[\\d_.xXa-fA-F]*\\b",
  ].map((s) => "(" + s + ")").join("|"), "g");
  const plain = new Set(["txt", "log", "nfo", "md", "markdown", "csv", "tsv", "srt", "vtt", ""]);

  function line(text, re) {
    const d = document.createElement("div");
    d.className = "fvl";
    if (re && text.length < 2000) {
      let i = 0;
      re.lastIndex = 0;
      for (let m; (m = re.exec(text)); ) {
        if (m[0] === "") { re.lastIndex++; continue; }
        if (m.index > i) d.append(text.slice(i, m.index));
        const s = document.createElement("span");
        s.className = m[1] ? "tc" : m[2] ? "ts" : "tn";
        s.textContent = m[0];
        d.append(s);
        i = m.index + m[0].length;
      }
      if (i < text.length) d.append(text.slice(i));
    } else d.append(text);
    d.append("\n");
    return d;
  }

  V.codeView = (ctx, text, { note } = {}) => {
    const { e, stage } = ctx;
    const x = V.ext(e.name);
    const re = plain.has(x) ? null : tokenRe(x);
    const lines = text.replace(/\r\n?/g, "\n").replace(/\n$/, "").split("\n");
    const code = h("div", { class: "fvtext fvcode", role: "region", "aria-label": e.name, tabindex: "0" });
    code.style.setProperty("--gw", String(String(lines.length).length + 1) + "ch");
    let shown = 0;
    const more = h("button", { class: "fvaction fvmorebtn", onclick: () => draw() });
    const draw = () => {
      const frag = document.createDocumentFragment();
      const end = Math.min(lines.length, shown + STEP);
      for (let i = shown; i < end; i++) frag.append(line(lines[i], re));
      code.append(frag);
      shown = end;
      more.hidden = shown >= lines.length;
      more.textContent = `Show more · ${(lines.length - shown).toLocaleString()} lines left`;
    };
    draw();
    const wrap = h("button", { class: "fvchip", "aria-pressed": String(wrapOn()), onclick: () => {
      const on = wrap.getAttribute("aria-pressed") !== "true";
      wrap.setAttribute("aria-pressed", String(on));
      code.classList.toggle("nowrap", !on);
      try { localStorage.setItem(WRAP, on ? "1" : "0"); } catch (_) {}
    } }, V.ico("wrap"), h("span", {}, "Wrap"));
    code.classList.toggle("nowrap", !wrapOn());
    const cut = note || (ctx.bytes && ctx.bytes.cut ? `Showing the first ${F.fmtSize(V.TEXT_MAX)} of ${F.fmtSize(e.size)} — download it for the rest.` : "");
    stage.replaceChildren(h("div", { class: "fvscroll" }, cut ? h("p", { class: "fvnote" }, cut) : null, code, more));
    ctx.tools(wrap);
    ctx.add("Text", [["Lines", lines.length.toLocaleString() + (ctx.bytes && ctx.bytes.cut ? "+" : "")]]);
    return null;
  };

  V.add("text", {
    fetch: "head",
    render(ctx) {
      ctx.bytes.blob.text().then((t) => { if (ctx.alive()) V.codeView(ctx, t); });
      return null;
    },
  });
})();
