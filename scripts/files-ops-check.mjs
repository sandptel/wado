// The file manager's editing in a real browser, with no computer behind it: the client's js/
// bundle (scripts/lib/bundle-page.mjs) over a small in-memory file tree that answers list, move,
// copy, rename and trash the way the daemon does (`{to}` in order; a clash refuses with "… exists"
// before anything moves). Wide layout, mouse and keyboard:
//
//   sort        Type (extension, then name), folders first or mixed in
//   clipboard   Ctrl+C / Ctrl+X hold items while browsing; Ctrl+V or "Paste here" puts them; a
//               clash asks, "Keep both" keeps both
//   undo        the toast's Undo and Ctrl+Z reverse a copy, a move, a duplicate and a rename
//   duplicate   Ctrl+D makes "name (1).ext" beside it
//   menus       right-click has Cut / Copy / Duplicate; several selected → Invert selection
//   keys        ↓ selects the next, Shift+↓ extends
//   grid        Ctrl+= / Ctrl+- change the tile size; it is kept
//   drag        a row dropped on a folder moves there; with Alt it copies
//
//   node scripts/files-ops-check.mjs        (needs Chrome: google-chrome-stable, or $CHROME)
import { spawn } from "node:child_process";
import { mkdtempSync, readFileSync, existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { bundle, bundlePage } from "./lib/bundle-page.mjs";

const MAC = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
const CHROME = process.env.CHROME || (existsSync(MAC) ? MAC : "google-chrome-stable");
const T = mkdtempSync(join(tmpdir(), "wado-files-ops-"));
const procs = [];
const start = (bin, args) => { const p = spawn(bin === "google-chrome-stable" ? CHROME : bin, args, { stdio: "ignore" }); procs.push(p); return p; };
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let fails = 0;
const check = (name, ok, more = "") => { if (!ok) fails++; console.log(`${ok ? "  ok  " : "FAIL  "}${name}${ok || !more ? "" : "\n        " + more}`); };

const page = await bundlePage({ dir: T, cdpPort: 9391, httpPort: 8791, start });
try {
  await page.cdp("Emulation.setDeviceMetricsOverride", { width: 1280, height: 800, deviceScaleFactor: 1, mobile: false });
  const css = readFileSync(new URL("../crates/client/assets/files.css", import.meta.url), "utf8");
  await page.ev(`(() => { const s = document.createElement("style"); s.textContent = ${JSON.stringify(css)}; document.head.append(s); return true; })()`);
  // The tree: path → { dir, size }. Answers as the daemon does.
  await page.ev(`(() => {
    const W = window.__wado, F = W.files;
    const fs = (window.__fs = new Map());
    for (const p of ["/h", "/h/a", "/h/a/sub", "/h/b", "/h/.Trash"]) fs.set(p, { dir: true, size: 0 });
    for (const p of ["/h/a/x.txt", "/h/a/y.png", "/h/a/z.md", "/h/a/Makefile", "/h/b/x.txt"]) fs.set(p, { dir: false, size: 10 });
    const parent = (p) => p.replace(/\\/[^/]*$/, "") || "/";
    const base = (p) => p.split("/").pop();
    const free = (dest, name) => { if (!fs.has(dest + "/" + name)) return dest + "/" + name;
      const m = /^(.*?)(\\.[^.]*)?$/.exec(name); for (let i = 1; ; i++) { const t = dest + "/" + m[1] + " (" + i + ")" + (m[2] || ""); if (!fs.has(t)) return t; } };
    const take = (from, to, keep) => { for (const [p, v] of [...fs]) if (p === from || p.startsWith(from + "/")) { fs.set(to + p.slice(from.length), v); if (!keep) fs.delete(p); } };
    F.connect = () => { F.ready = true; F.info = { roots: ["/h"], home: "/h", access: "rw", space: [], trash: "/h/.Trash" }; return Promise.resolve(F.info); };
    F.req = async (op, a = {}) => {
      await new Promise((r) => setTimeout(r, 5));
      if (op === "quick") return { pins: [], recent: [] };
      if (op === "list") return { entries: [...fs].filter(([p]) => parent(p) === a.path && p !== a.path).map(([p, v]) => ({ name: base(p), dir: v.dir, size: v.size, mtime: 1 })) };
      if (op === "move" || op === "copy") {
        if (a.clash === "fail") for (const p of a.paths) if (fs.has(a.dest + "/" + base(p))) throw new Error(base(p) + " exists");
        return { to: a.paths.map((p) => { const t = a.clash === "replace" ? a.dest + "/" + base(p) : free(a.dest, base(p)); take(p, t, op === "copy"); return t; }) };
      }
      if (op === "rename") { const t = parent(a.path) + "/" + a.to; if (fs.has(t)) throw new Error(a.to + " exists"); take(a.path, t); return {}; }
      if (op === "trash") { for (const p of a.paths) take(p, free("/h/.Trash", base(p))); return { to: [] }; }
      return {};
    };
    F.ui.path = "/h/a";
    W.filesOpen();
    return true;
  })()`);
  const until = async (expr, ms = 4000) => { const t = Date.now(); while (Date.now() - t < ms) { if (await page.ev(expr).catch(() => false)) return true; await sleep(50); } return false; };
  const has = (p) => page.ev(`window.__fs.has(${JSON.stringify(p)})`);
  const names = () => page.ev(`[...document.querySelectorAll("#wado-files .frow[data-i] .fname")].map((n) => n.firstChild.textContent)`);
  const row = (name) => `[...document.querySelectorAll("#wado-files [data-i]")].find((r) => r.querySelector(".fname").firstChild.textContent === ${JSON.stringify(name)})`;
  const click = (name, init = {}) => page.ev(`(() => { ${row(name)}.dispatchEvent(new MouseEvent("click", { bubbles: true, ...${JSON.stringify(init)} })); return true; })()`);
  const open = (name) => page.ev(`(() => { ${row(name)}.dispatchEvent(new MouseEvent("dblclick", { bubbles: true })); return true; })()`);
  // Keys go where the browser would send them: the focused element, which must be in the window.
  const key = (k, init = {}) => page.ev(`(() => { const a = document.activeElement; if (!document.getElementById("wado-files").contains(a)) return "focus is on " + a.tagName;
    a.dispatchEvent(new KeyboardEvent("keydown", { key: ${JSON.stringify(k)}, bubbles: true, ...${JSON.stringify(init)} })); return true; })()`);
  const sheet = () => page.ev(`[...document.querySelectorAll(".wado-prompt .menurow")].map((b) => b.textContent)`);
  const choose = (label) => page.ev(`(() => { const b = [...document.querySelectorAll(".wado-prompt .menurow")].find((b) => b.textContent === ${JSON.stringify(label)}); if (!b) return false; b.click(); return true; })()`);
  const toast = () => page.ev(`[...document.querySelectorAll(".ftoast")].map((t) => t.textContent).join(" | ")`);
  const undoBtn = () => page.ev(`(() => { const b = document.querySelector(".ftoast.act button"); if (!b) return false; b.click(); return true; })()`);
  const bar = () => page.ev(`document.querySelector("#wado-files .fpickbar")?.textContent || ""`);
  const sel = () => page.ev(`[...window.__wado.files.ui.sel].map((p) => p.split("/").pop())`);
  const settle = async () => { await sleep(150); await page.ev(`document.querySelectorAll(".ftoast").forEach((t) => t.remove()); true`); };

  check("opens in /h/a, wide", await until(`document.querySelector('#wado-files[data-layout="wide"] .frow[data-i]') !== null`));

  // sort
  await page.ev(`document.querySelector("#wado-files .ftools .fchip").click(); true`);
  check("the sort sheet offers Type and folder placement", (await sheet()).includes("Type") && (await sheet()).includes("Mix folders with files"), JSON.stringify(await sheet()));
  await choose("Type");
  check("Type: folders first, then by extension", JSON.stringify(await names()) === JSON.stringify(["sub", "Makefile", "z.md", "y.png", "x.txt"]), JSON.stringify(await names()));
  await page.ev(`document.querySelector("#wado-files .ftools .fchip").click(); true`);
  await choose("Mix folders with files");
  check("mixed: the folder sorts with the files", JSON.stringify(await names()) === JSON.stringify(["Makefile", "sub", "z.md", "y.png", "x.txt"]), JSON.stringify(await names()));
  check("the choice is kept", await page.ev(`(() => { const v = JSON.parse(localStorage.getItem("wado.files.view")); return v.sort.by === "type" && v.dirsFirst === false; })()`));
  check("the wide list has a Type column", await page.ev(`!!document.querySelector("#wado-files .fhd .c-type") && ${row("z.md")}.querySelector(".c-type").textContent === "MD"`));

  // clipboard: copy, clash, keep both, undo
  await click("x.txt");
  check("Ctrl+C with x.txt selected", (await key("c", { ctrlKey: true })) === true);
  check("the bar holds it", /Copy 1 item into a/.test(await bar()) && /Paste here/.test(await bar()), await bar());
  await key("Backspace");
  await until(`!!(${row("b")})`);
  await open("b");
  await until(`window.__wado.files.ui.path === "/h/b" && !window.__wado.files.ui.loading`);
  check("…and still holds it in another folder", /Copy 1 item into b/.test(await bar()), await bar());
  await key("v", { ctrlKey: true });
  check("Ctrl+V on a clash asks", await until(`[...document.querySelectorAll(".wado-prompt .menurow")].some((b) => b.textContent === "Keep both")`));
  await choose("Keep both");
  check("Keep both: a second copy", await until(`window.__fs.has("/h/b/x (1).txt")`) && await has("/h/b/x.txt"));
  check("the toast says so, with Undo", /Copied 1 item/.test(await toast()) && await until(`!!document.querySelector(".ftoast.act button")`), await toast());
  check("the bar is gone after the paste", (await bar()) === "");
  await undoBtn();
  check("Undo: the copy goes to the Trash", await until(`!window.__fs.has("/h/b/x (1).txt") && window.__fs.has("/h/.Trash/x (1).txt")`));
  await settle();

  // clipboard: cut, Paste here, Ctrl+Z
  await key("Backspace");
  await until(`!!(${row("a")})`);
  await open("a");
  await until(`window.__wado.files.ui.path === "/h/a" && !!(${row("y.png")})`);
  await click("y.png");
  await key("x", { ctrlKey: true });
  check("Ctrl+X: Move 1 item", /Move 1 item into a/.test(await bar()), await bar());
  await click("sub");
  check("with a clipboard, items still select", (await sel()).join() === "sub" && !!(await page.ev(`document.querySelector("#wado-files .fselbar")`)));
  await key("Escape");
  check("Esc clears the selection first, the clipboard stays", (await sel()).length === 0 && /Move 1 item/.test(await bar()), await bar());
  await open("sub");
  await until(`window.__wado.files.ui.path === "/h/a/sub"`);
  await page.ev(`[...document.querySelectorAll("#wado-files .fpickbar .fgo")].find((b) => b.textContent === "Paste here").click(); true`);
  check("Paste here moves it", await until(`window.__fs.has("/h/a/sub/y.png") && !window.__fs.has("/h/a/y.png")`));
  await sleep(100);
  await key("z", { ctrlKey: true });
  check("Ctrl+Z moves it back", await until(`window.__fs.has("/h/a/y.png") && !window.__fs.has("/h/a/sub/y.png")`));
  await settle();
  await key("Backspace");
  await until(`window.__wado.files.ui.path === "/h/a" && !!(${row("z.md")})`);

  // duplicate
  await click("z.md");
  await key("d", { ctrlKey: true });
  check("Ctrl+D duplicates beside it", await until(`window.__fs.has("/h/a/z (1).md")`) && /Duplicated “z.md”/.test(await toast()), await toast());
  await undoBtn();
  check("…and Undo trashes the duplicate", await until(`!window.__fs.has("/h/a/z (1).md")`));
  await settle();

  // rename + undo
  await until(`!!(${row("z.md")})`);
  await click("z.md");
  await key("F2");
  await until(`!!document.querySelector(".fmodal .finput")`);
  await page.ev(`(() => { const i = document.querySelector(".fmodal .finput"); i.value = "notes.md"; i.form.requestSubmit(); return true; })()`);
  check("F2 renames", await until(`window.__fs.has("/h/a/notes.md")`));
  await sleep(100);
  await page.ev(`document.getElementById("wado-files").focus(); true`);
  await key("z", { ctrlKey: true });
  check("Ctrl+Z renames it back", await until(`window.__fs.has("/h/a/z.md") && !window.__fs.has("/h/a/notes.md")`));
  await settle();
  await until(`!!(${row("z.md")})`);

  // menus
  await page.ev(`(() => { ${row("x.txt")}.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true })); return true; })()`);
  const items = await sheet();
  check("right-click: Cut, Copy, Duplicate", ["Cut", "Copy", "Duplicate", "Copy to…", "Move to…"].every((l) => items.includes(l)), JSON.stringify(items));
  await choose("Cancel");
  await click("y.png", { ctrlKey: true });
  await page.ev(`(() => { ${row("y.png")}.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true })); return true; })()`);
  check("right-click on two: their own sheet", (await page.ev(`document.querySelector(".wado-prompt .sheettitle")?.textContent`)) === "2 selected");
  await choose("Invert selection");
  check("Invert selection", JSON.stringify((await sel()).sort()) === JSON.stringify(["Makefile", "sub", "z.md"]), JSON.stringify(await sel()));
  await key("a", { ctrlKey: true, shiftKey: true });
  check("Ctrl+Shift+A selects none", (await sel()).length === 0);

  // arrows: from the last clicked
  await click("Makefile");
  await key("ArrowDown");
  check("↓ selects the next", JSON.stringify(await sel()) === JSON.stringify(["sub"]), JSON.stringify(await sel()));
  await key("ArrowDown", { shiftKey: true });
  check("↓ then Shift+↓ extends", JSON.stringify(await sel()) === JSON.stringify(["sub", "z.md"]), JSON.stringify(await sel()));
  check("the cursor row has the focus", await page.ev(`document.activeElement === ${row("z.md")}`));
  await key("Escape");

  // grid size
  await page.ev(`document.querySelector("#wado-files .fviewbtn").click(); true`);
  await until(`!!document.querySelector("#wado-files .fgrid")`);
  await page.ev(`document.getElementById("wado-files").focus(); true`);
  const cols = () => page.ev(`getComputedStyle(document.querySelector("#wado-files .fgrid")).gridTemplateColumns.split(" ").length`);
  const before = await cols();
  await key("=", { ctrlKey: true });
  check("Ctrl+= makes the tiles larger", (await page.ev(`document.querySelector("#wado-files .fgrid").classList.contains("t-l")`)) && (await cols()) < before, `${before} → ${await cols()}`);
  await key("-", { ctrlKey: true }); await key("-", { ctrlKey: true });
  check("Ctrl+- twice: small", (await page.ev(`window.__wado.files.ui.tile`)) === "s" && (await cols()) > before);
  check("the size is kept", await page.ev(`JSON.parse(localStorage.getItem("wado.files.view")).tile === "s"`));
  await page.ev(`document.querySelector("#wado-files .fviewbtn").click(); true`);
  await until(`!!document.querySelector("#wado-files .flist")`);

  // drag
  const drag = (name, onto, alt) => page.ev(`(() => {
    const dt = new DataTransfer(), src = ${row(name)}, dst = ${row(onto)};
    if (src.getAttribute("draggable") !== "true") return "not draggable";
    src.dispatchEvent(new DragEvent("dragstart", { bubbles: true, dataTransfer: dt }));
    const over = new DragEvent("dragover", { bubbles: true, cancelable: true, dataTransfer: dt, altKey: ${!!alt} });
    dst.dispatchEvent(over);
    const lit = dst.classList.contains("fdropon");
    dst.dispatchEvent(new DragEvent("drop", { bubbles: true, cancelable: true, dataTransfer: dt, altKey: ${!!alt} }));
    src.dispatchEvent(new DragEvent("dragend", { bubbles: true, dataTransfer: dt }));
    return over.defaultPrevented && lit;
  })()`);
  check("a folder takes a dragged row, lit", (await drag("x.txt", "sub")) === true);
  check("dropped: moved into it", await until(`window.__fs.has("/h/a/sub/x.txt") && !window.__fs.has("/h/a/x.txt")`) && /Moved 1 item to sub/.test(await toast()), await toast());
  await settle();
  await until(`!!(${row("z.md")})`);
  check("with Alt: copied", (await drag("z.md", "sub", true)) === true && await until(`window.__fs.has("/h/a/sub/z.md") && window.__fs.has("/h/a/z.md")`));
  check("a folder is not its own target", (await page.ev(`(() => { const dt = new DataTransfer(), s = ${row("sub")};
    s.dispatchEvent(new DragEvent("dragstart", { bubbles: true, dataTransfer: dt }));
    const o = new DragEvent("dragover", { bubbles: true, cancelable: true, dataTransfer: dt }); s.dispatchEvent(o);
    s.dispatchEvent(new DragEvent("dragend", { bubbles: true, dataTransfer: dt })); return o.defaultPrevented; })()`)) === false);

  // An "inline" part of bridge.rs that the bundler splits badly fails to parse at HEAD too; not ours.
  const parts = bundle();
  const errs = (await page.errors()).filter((e) => { const m = /\/b\/(\d+)\.js/.exec(e); return !m || parts[+m[1]].name !== "inline"; });
  check("no script errors", !errs.length, errs.join("\n"));
} finally {
  page.close();
  for (const p of procs) try { p.kill("SIGTERM"); } catch {}
}
console.log(fails ? `${fails} failed` : "all ok");
process.exit(fails ? 1 : 0);
