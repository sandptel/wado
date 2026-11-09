// The file manager's pure parts (crates/client/src/js/files_ui.js), fed by hand in Node: the
// listing's order (name, size, type; folders first or mixed) and the undo plan of a move, copy,
// duplicate or rename. Exits non-zero on a failure.
//   node scripts/files-order-check.mjs
import { readFileSync } from "node:fs";
import assert from "node:assert/strict";

const src = readFileSync(new URL("../crates/client/src/js/files_ui.js", import.meta.url), "utf8");
const W = { files: { onXfer() {}, onDown() {} }, relayOn() {} };
new Function("W", "window", "localStorage", src)(W, {}, undefined);
const F = W.files;

const list = [
  { name: "z.png", size: 3 }, { name: "a.TXT", size: 1 }, { name: "docs", dir: true }, { name: "b.png", size: 3 },
  { name: "README", size: 9 }, { name: "x.tar.gz", size: 2 }, { name: "Makefile", size: 9 }, { name: "art", dir: true },
];
const names = (by, desc, dirsFirst) => list.slice().sort(F.order({ by, desc }, dirsFirst)).map((e) => e.name);

// type: by extension (none first), then name; folders stay first, also when reversed
assert.deepEqual(names("type", false, true), ["art", "docs", "Makefile", "README", "x.tar.gz", "b.png", "z.png", "a.TXT"]);
assert.deepEqual(names("type", true, true), ["docs", "art", "a.TXT", "z.png", "b.png", "x.tar.gz", "README", "Makefile"]);
// folders mixed in
assert.deepEqual(names("name", false, false), ["a.TXT", "art", "b.png", "docs", "Makefile", "README", "x.tar.gz", "z.png"]);
assert.deepEqual(names("name", false, true).slice(0, 2), ["art", "docs"]);
// size: ties broken by name, which turns with the order
assert.deepEqual(names("size", true, true), ["docs", "art", "README", "Makefile", "z.png", "b.png", "x.tar.gz", "a.TXT"]);
assert.deepEqual(names("size", false, true), ["art", "docs", "a.TXT", "x.tar.gz", "b.png", "z.png", "Makefile", "README"]);

// undo a move: each landed path back to the folder it came from, keeping both on a clash
assert.deepEqual(F.undoPlan("move", ["/h/a/x", "/h/b/y", "/h/a/z", "/r"], ["/h/d/x", "/h/d/y (1)", "/h/d/z", "/h/d/r"]), [
  ["move", { paths: ["/h/d/x", "/h/d/z"], dest: "/h/a", clash: "rename" }],
  ["move", { paths: ["/h/d/y (1)"], dest: "/h/b", clash: "rename" }],
  ["move", { paths: ["/h/d/r"], dest: "/", clash: "rename" }],
]);
// undo a copy or a duplicate: the copies go to the Trash
assert.deepEqual(F.undoPlan("copy", ["/h/a.txt"], ["/h/a (1).txt"]), [["trash", { paths: ["/h/a (1).txt"] }]]);
// undo a rename: back to the old name
assert.deepEqual(F.undoPlan("rename", ["/h/old.txt"], ["/h/new.txt"]), [["rename", { path: "/h/new.txt", to: "old.txt" }]]);

console.log("files-order-check: ok");
