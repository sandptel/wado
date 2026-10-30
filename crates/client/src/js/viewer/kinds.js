// wado bridge — the viewer's dispatcher: what kind of file a name is, what to call it, and which
// viewer module draws it. Each kind's module (js/viewer/<kind>.js) registers itself here.
//
//   W.files.viewable(name) → kind            every file has one; "info" when nothing previews it
//   W.files.viewer.kind(name) / ext(name) / mime(name) / type(name)   ("PNG image", "Rust source")
//   W.files.viewer.add(kind, def)            a module: { fetch, limit?, icon, render(ctx) }
//   W.files.viewer.get(kind) → def
//
// `fetch` says what the module needs before it draws: "whole" (the file's bytes), "head" (the
// first `limit` bytes — text), "none" (it asks for what it needs itself: streams, listings).
// The limits — what is fetched before asking, what the relay's limited lane carries — are here
// too, so every module answers to the same ones.

(() => {
  const F = W.files;
  const V = (F.viewer = F.viewer || {});

  const KINDS = {
    image: "png jpg jpeg gif webp avif bmp ico svg heic heif",
    // Any format ffmpeg reads: the computer converts what the browser cannot play.
    video: "mp4 m4v webm mov mkv ogv avi wmv flv mpg mpeg ts m2ts mts 3gp vob divx rmvb asf",
    audio: "mp3 m4a aac ogg oga opus flac wav weba wma ape alac aiff aif mka ac3 dts amr",
    pdf: "pdf",
    markdown: "md markdown mdown",
    json: "json jsonl geojson webmanifest",
    csv: "csv tsv",
    text: "txt log nfo xml yaml yml toml ini conf cfg kdl nix rs js mjs cjs ts tsx jsx py go c h cc cpp hpp java kt rb php sh "
      + "fish zsh bash lua sql swift zig hs ml css scss html htm srt vtt diff patch env gitignore dockerfile makefile",
    archive: "zip jar apk epub docx xlsx pptx odt ods odp tar gz tgz xz bz2 zst 7z rar",
    font: "ttf otf woff woff2",
  };
  const BY = {};
  for (const [k, exts] of Object.entries(KINDS)) for (const x of exts.split(" ")) BY[x] = k;
  // Names with no extension that are text all the same.
  const BARE = { makefile: "text", dockerfile: "text", readme: "text", license: "text", changelog: "text" };

  const MIME = {
    png: "image/png", jpg: "image/jpeg", jpeg: "image/jpeg", gif: "image/gif", webp: "image/webp", avif: "image/avif",
    bmp: "image/bmp", ico: "image/x-icon", svg: "image/svg+xml", heic: "image/heic", heif: "image/heif",
    mp4: "video/mp4", m4v: "video/mp4", webm: "video/webm", mov: "video/mp4", mkv: "video/webm", ogv: "video/ogg",
    mp3: "audio/mpeg", m4a: "audio/mp4", aac: "audio/aac", ogg: "audio/ogg", oga: "audio/ogg", opus: "audio/ogg",
    flac: "audio/flac", wav: "audio/wav", weba: "audio/webm", pdf: "application/pdf",
    ttf: "font/ttf", otf: "font/otf", woff: "font/woff", woff2: "font/woff2",
  };
  const SOURCE = new Set("rs js mjs cjs ts tsx jsx py go c h cc cpp hpp java kt rb php sh fish zsh bash lua sql swift zig hs ml css scss html htm nix kdl".split(" "));
  const NAMED = { txt: "Text", log: "Log", nfo: "Text", md: "Markdown", markdown: "Markdown", mdown: "Markdown", json: "JSON",
    jsonl: "JSON lines", csv: "CSV table", tsv: "TSV table", pdf: "PDF document", diff: "Patch", patch: "Patch",
    yaml: "YAML", yml: "YAML", toml: "TOML", xml: "XML", ini: "Settings", conf: "Settings", cfg: "Settings", env: "Settings",
    srt: "Subtitles", vtt: "Subtitles", epub: "EPUB book", docx: "Word document", xlsx: "Excel sheet", pptx: "Slides",
    odt: "Document", ods: "Spreadsheet", odp: "Slides", apk: "Android app", jar: "Java archive" };

  V.ext = (name) => { const i = name.lastIndexOf("."); return i > 0 ? name.slice(i + 1).toLowerCase() : ""; };
  V.kind = (name) => BY[V.ext(name)] || BARE[name.toLowerCase()] || "info";
  V.mime = (name) => MIME[V.ext(name)] || (V.kind(name) === "text" || V.kind(name) === "markdown" ? "text/plain" : "application/octet-stream");
  V.type = (name) => {
    const x = V.ext(name), k = V.kind(name), X = x.toUpperCase();
    if (NAMED[x]) return NAMED[x];
    if (k === "image" || k === "video" || k === "audio") return `${X} ${k}`;
    if (k === "text") return SOURCE.has(x) ? `${X} source` : x ? `${X} text` : "Text";
    if (k === "archive") return `${X} archive`;
    if (k === "font") return `${X} font`;
    return x ? `${X} file` : "File";
  };

  // What is fetched before asking: past this, "this is big — load it?"
  V.ASK_OVER = 300 << 20;
  // Text-like files are read up to this much; the rest is cut, and said so.
  V.TEXT_MAX = 2 << 20;
  // On the relay's limited lane (the fallback when there is no direct path) only listings and
  // files this small come through; bigger previews wait for Direct, and the viewer says so.
  V.LANE_MAX = 1 << 20;
  // `W.files.lane` is true while the files channel rides that lane (js/files_link.js).
  V.lane = () => (F.lane ? "fallback" : "direct");

  const DEFS = {};
  V.add = (kind, def) => { DEFS[kind] = def; };
  V.get = (kind) => DEFS[kind] || DEFS.info;

  // Every file opens in the viewer: what cannot be previewed gets an honest card there.
  F.viewable = (name) => V.kind(name);
})();
