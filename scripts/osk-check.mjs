// Runnable check for the soft-keyboard open/close rules in js/osk.js.
//
// The branch worth pinning is the asymmetry: a keyboard the *compositor* raised (an app focused
// a text field) should close when that app is done, but one the *viewer* raised with ⌨ must not
// be closed out from under them by a toolkit emitting `disable` during focus churn. Getting that
// backwards produces one of the two complaints this feature exists to remove.
//
// Run:  node scripts/osk-check.mjs
import { readFileSync } from "node:fs";

const src = readFileSync(new URL("../crates/client/src/js/osk.js", import.meta.url), "utf8");

// Enough of a DOM for ensureInput()/focus()/blur(). The real one is exercised by a human.
function makeEnv() {
  const listeners = {};
  const input = {
    id: "", type: "", value: "",
    setAttribute() {}, click() {},
    focus() { env.document.activeElement = input; },
    blur() { if (env.document.activeElement === input) env.document.activeElement = null; },
    addEventListener(k, f) { listeners[k] = f; },
  };
  const env = {
    document: {
      activeElement: null,
      body: { appendChild() {} },
      createElement: () => input,
      addEventListener() {},
    },
    addEventListener() {},
    input,
  };
  return env;
}

let failures = 0;
const run = (name, steps) => {
  const env = makeEnv();
  const W = { sendInput() {} };
  const emit = () => {};
  globalThis.document = env.document;
  globalThis.addEventListener = env.addEventListener;
  new Function("W", "emit", src)(W, emit);
  const up = () => env.document.activeElement === env.input;
  try {
    steps(W, up);
    console.log(`ok   ${name}`);
  } catch (e) {
    failures++;
    console.log(`FAIL ${name}: ${e.message}`);
  }
};
const assert = (cond, msg) => { if (!cond) throw new Error(msg); };

run("an app focusing a text field raises the keyboard", (W, up) => {
  assert(!up(), "keyboard should start down");
  W.textInput(true);
  assert(up(), "text input active should raise it");
});

run("the app giving up text input closes what it opened", (W, up) => {
  W.textInput(true);
  W.textInput(false);
  assert(!up(), "auto-opened keyboard should close when the app is done");
});

run("a keyboard the viewer opened survives a spurious disable", (W, up) => {
  W.oskToggle();                 // the viewer pressed ⌨
  assert(up(), "toggle should raise it");
  W.textInput(false);            // toolkit churn says "no text input"
  assert(up(), "a manually opened keyboard must NOT be auto-closed");
});

run("the viewer can still close a keyboard the app raised", (W, up) => {
  W.textInput(true);
  W.oskToggle();
  assert(!up(), "toggle should close it");
});

run("repeated activation does not thrash focus", (W, up) => {
  W.textInput(true);
  const before = globalThis.document.activeElement;
  W.textInput(true);
  assert(globalThis.document.activeElement === before && up(), "second activate should be a no-op");
});

console.log(failures ? `\n${failures} FAILED` : "\nall passed");
process.exit(failures ? 1 : 0);
