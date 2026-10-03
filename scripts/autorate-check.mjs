// Runnable check for js/autorate.js: the cap falls on a congested link, holds, and climbs back.
//   node scripts/autorate-check.mjs
import { readFileSync } from "node:fs";
const src = readFileSync(new URL("../crates/client/src/js/autorate.js", import.meta.url), "utf8");
const W = { setTargetKbps: () => {} };
const emitted = [];
new Function("W", "emit", src)(W, (m) => emitted.push(m.kbps));
let failures = 0;
const check = (name, ok, got) => { if (!ok) failures++; console.log(`${ok ? "ok  " : "FAIL"} ${name}${ok ? "" : " — got " + JSON.stringify(got)}`); };
const a = W.autorate;
W.setTargetKbps(3941);
let t = 0;
const run = (n, s) => { let out; for (let i = 0; i < n; i++) { t += 1000; const r = a.feed(s, t); if (r !== undefined) out = r; } return out; };
run(10, { ping: 30, jbuf: 20, kbps: 3800, lossPct: 0 });                 // a good link learns its floor
check("a clean link is left alone", a.cap === null, a.cap);
const down = run(4, { ping: 300, jbuf: 270, kbps: 900, lossPct: 0 });    // the cellular collapse
check("congestion caps it near what arrived, at most halving", down >= 1970 && down <= 2760 || down === Math.round(Math.max(3941 * 0.5, Math.min(3941 * 0.7, 900 * 0.9))), down);
const first = a.cap;
run(3, { ping: 300, jbuf: 270, kbps: 700, lossPct: 0 });
check("no second step inside the 6 s gap", a.cap === first, a.cap);
run(8, { ping: 300, jbuf: 270, kbps: 700, lossPct: 0 });
check("still congested later: steps down again", a.cap < first, a.cap);
run(80, { ping: 30, jbuf: 20, kbps: 600, lossPct: 0 });
check("a clean link climbs back to the full rate", a.cap === null && emitted.at(-1) === null, [a.cap, emitted]);
a.reset(); W.setTargetKbps(3941); a.enabled = false;
check("off: never touches it", run(10, { ping: 500, jbuf: 400, kbps: 100, lossPct: 9 }) === undefined && a.cap === null, a.cap);
console.log(failures ? `\n${failures} failed` : "\nall passed");
process.exit(failures ? 1 : 0);
