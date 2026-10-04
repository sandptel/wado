// wado bridge — the file manager's icons: one stroked path each, like ui/widgets/icon.rs, so
// `currentColor` themes them. A file's icon is its MIME *family*, read from the extension, and
// each family has a hue from the base16 palette (`--hue` on the icon's disc).
//
//   W.files.icon(name)                  svg markup for a UI icon
//   W.files.kind(name, isDir) → { icon, hue, image }

(() => {
  const F = W.files;
  const P = {
    folder: "M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z",
    file: "M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8zM14 3v5h5",
    image: "M5 4h14a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2zM8.5 9.5h.01M21 15l-5-5L5 20",
    video: "M4 6h11a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2zM17 10l5-3v10l-5-3",
    audio: "M9 18V5l12-2v13M9 18a3 3 0 1 1-6 0a3 3 0 1 1 6 0zM21 16a3 3 0 1 1-6 0a3 3 0 1 1 6 0z",
    archive: "M4 4h16v5H4zM5 9v11h14V9M10 13h4",
    pdf: "M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8zM14 3v5h5M8 13h2.5a1.5 1.5 0 0 1 0 3H8v-3zm0 3v2",
    doc: "M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8zM14 3v5h5M9 13h6M9 17h6",
    sheet: "M5 3h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2zM3 9h18M3 15h18M9 3v18",
    slides: "M3 4h18v12H3zM12 16v4M8 20h8",
    code: "M8 8l-4 4 4 4M16 8l4 4-4 4M14 5l-4 14",
    app: "M12 2l9 5v10l-9 5-9-5V7zM12 22V12M21 7l-9 5-9-5",
    // UI
    back: "M15 18l-6-6 6-6",
    up: "M12 19V5M5 12l7-7 7 7",
    down: "M12 5v14M5 12l7 7 7-7",
    upload: "M12 16V4M7 9l5-5 5 5M4 20h16",
    download: "M12 4v12M7 11l5 5 5-5M4 20h16",
    x: "M18 6 6 18M6 6l12 12",
    plus: "M12 5v14M5 12h14",
    dots: "M12 5h.01M12 12h.01M12 19h.01",
    grid: "M4 4h6v6H4zM14 4h6v6h-6zM4 14h6v6H4zM14 14h6v6h-6z",
    list: "M8 6h13M8 12h13M8 18h13M3 6h.01M3 12h.01M3 18h.01",
    sort: "M3 6h18M6 12h12M10 18h4",
    pin: "M12 17v5M9 3h6l-1 6 4 4H6l4-4z",
    trash: "M3 6h18M8 6V4h8v2M6 6l1 14h10l1-14",
    edit: "M12 20h9M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4z",
    copy: "M9 9h11v11H9zM5 15H4V4h11v1",
    move: "M5 9l-3 3 3 3M2 12h14M13 5h7v14h-7",
    newfolder: "M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2zM12 10v6M9 13h6",
    check: "M20 6 9 17l-5-5",
    pause: "M7 4h3v16H7zM14 4h3v16h-3z",
    play: "M7 4v16l13-8z",
    clock: "M12 3a9 9 0 1 0 0 18a9 9 0 1 0 0-18zM12 7v5l3 2",
    home: "M3 11l9-8 9 8M5 10v10h14V10",
    drive: "M3 13h18v6H3zM5 13l3-8h8l3 8M7 16h.01",
    swap: "M7 4v16M3 8l4-4 4 4M17 20V4M13 16l4 4 4-4",
    transfers: "M4 14v4a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-4M8 9l4 4 4-4M12 3v10",
    lock: "M6 11h12a2 2 0 0 1 2 2v6a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2v-6a2 2 0 0 1 2-2zM8 11V7a4 4 0 0 1 8 0v4",
    devices: "M7 2h10a2 2 0 0 1 2 2v16a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2zM11 18h2",
    selectall: "M4 4h16v16H4zM8 12l3 3 5-6",
    sliders: "M4 21v-7M4 10V3M12 21v-9M12 8V3M20 21v-5M20 12V3M1 14h6M9 8h6M17 16h6",
    search: "M11 4a7 7 0 1 0 0 14a7 7 0 1 0 0-14zM21 21l-5-5",
    undo: "M9 14 4 9l5-5M4 9h11a5 5 0 0 1 0 10h-3",
    monitor: "M4 3h16a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2zM8 21h8M12 17v4",
    eye: "M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7S2 12 2 12zM12 9a3 3 0 1 0 0 6a3 3 0 1 0 0-6z",
  };
  F.icon = (name) =>
    `<svg class="i" viewBox="0 0 24 24" aria-hidden="true"><path d="${P[name] || P.file}"/></svg>`;

  const FAM = [
    ["image", "--base0E", "png jpg jpeg gif webp bmp tif tiff svg avif heic ico raw cr2 nef"],
    ["video", "--base08", "mp4 mkv webm mov avi m4v wmv flv mpg mpeg 3gp"],
    ["audio", "--base0C", "mp3 flac ogg opus wav m4a aac wma aiff mid"],
    ["archive", "--base09", "zip tar gz tgz xz bz2 zst 7z rar iso deb rpm"],
    ["pdf", "--base08", "pdf epub djvu"],
    ["sheet", "--base0B", "xls xlsx ods csv tsv"],
    ["slides", "--base09", "ppt pptx odp key"],
    ["doc", "--base0D", "txt md doc docx odt rtf tex org log nfo"],
    ["code", "--base0A", "rs js mjs ts tsx jsx py go c h cc cpp hpp java kt rb php sh fish zsh bash lua nix toml yaml yml json kdl xml html css scss sql swift zig hs ml"],
    ["app", "--base0F", "exe msi apk appimage dmg flatpakref bin run"],
  ];
  const BY_EXT = {};
  for (const [icon, hue, exts] of FAM) for (const e of exts.split(" ")) BY_EXT[e] = { icon, hue: `var(${hue})` };

  F.kind = (name, isDir) => {
    if (isDir) return { icon: "folder", hue: "var(--base0D)", image: false };
    const dot = name.lastIndexOf(".");
    const ext = dot > 0 ? name.slice(dot + 1).toLowerCase() : "";
    const k = BY_EXT[ext] || { icon: "file", hue: "var(--dim)" };
    return { ...k, image: k.icon === "image" };
  };
})();
