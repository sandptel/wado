// wado bridge — on-screen keyboard for touch devices.
//
// A phone has no keys. The Wayland answer is `zwp_text_input_v3`, which is inert without an
// `zwp_input_method_v2` client bound (smithay drops every text-input request when no IME
// instance exists), so it would be a global apps bind and get nothing from. wado already
// injects keys directly, so the whole problem is "make the phone's keyboard appear".
//
// Only a focused editable element raises a soft keyboard, so there is a real input here rather
// than a synthetic event. It is visually hidden but NOT `display:none`, `hidden`, or
// `opacity:0` with zero size — browsers refuse to raise the keyboard for an element they
// consider invisible. One character tall, clipped, parked under the bar.
//
// ⚠️ **Android soft keyboards do not report `KeyboardEvent.code`.** It is `""` for ordinary
// characters, so the existing `code`-based mapper (`input_keyboard.js`) sees nothing typable.
// Characters therefore come from the `input` event and are translated here; only the keys
// Android *does* report as codes (Enter, Backspace, arrows) go through `keydown`.

// US layout, unshifted. Derived from the same evdev numbers as `input_keyboard.js`.
const CHAR_TO_EVDEV = {
  a:30,b:48,c:46,d:32,e:18,f:33,g:34,h:35,i:23,j:36,k:37,l:38,m:50,
  n:49,o:24,p:25,q:16,r:19,s:31,t:20,u:22,v:47,w:17,x:45,y:21,z:44,
  "1":2,"2":3,"3":4,"4":5,"5":6,"6":7,"7":8,"8":9,"9":10,"0":11,
  " ":57,"-":12,"=":13,"[":26,"]":27,"\\":43,";":39,"'":40,"`":41,",":51,".":52,"/":53,
};
// Characters that need shift, mapped to the unshifted key that produces them.
const SHIFTED = {
  "!":"1","@":"2","#":"3","$":"4","%":"5","^":"6","&":"7","*":"8","(":"9",")":"0",
  "_":"-","+":"=","{":"[","}":"]","|":"\\",":":";",'"':"'","~":"`","<":",",">":".","?":"/",
};
const SHIFT_EVDEV = 42; // ShiftLeft

let el = null;

function ensureInput() {
  if (el) return el;
  el = document.createElement("input");
  el.id = "wado-osk";
  el.type = "text";
  // Everything that stops a phone "helping": no autocorrect rewriting what was typed, no
  // capitalisation on the first letter, no completion popover stealing the next tap.
  el.setAttribute("autocomplete", "off");
  el.setAttribute("autocorrect", "off");
  el.setAttribute("autocapitalize", "none");
  el.setAttribute("spellcheck", "false");
  el.setAttribute("aria-label", "Keyboard input for the remote session");
  document.body.appendChild(el);

  el.addEventListener("input", () => {
    const text = el.value;
    el.value = "";                     // never accumulate; each char is sent once
    for (const ch of text) sendChar(ch);
  });

  // Backspace and Enter arrive as codes even on Android, and must not be swallowed by the
  // input's own editing behaviour — the field is always empty, so a Backspace there does
  // nothing locally and would otherwise be lost.
  el.addEventListener("keydown", (e) => {
    const named = { Enter: 28, Backspace: 14, Tab: 15, Escape: 1,
                    ArrowUp: 103, ArrowDown: 108, ArrowLeft: 105, ArrowRight: 106 }[e.key];
    if (named === undefined) return;
    e.preventDefault();
    tap(named);
  });

  // Whatever closed the keyboard (back gesture, another field) leaves the toggle honest.
  el.addEventListener("blur", () => W.oskState(false));
  return el;
}

function tap(code, withShift) {
  if (withShift) W.sendInput({ t: "key", code: SHIFT_EVDEV, pressed: true });
  W.sendInput({ t: "key", code, pressed: true });
  W.sendInput({ t: "key", code, pressed: false });
  if (withShift) W.sendInput({ t: "key", code: SHIFT_EVDEV, pressed: false });
}

function sendChar(ch) {
  const lower = ch.toLowerCase();
  if (SHIFTED[ch] !== undefined) return tap(CHAR_TO_EVDEV[SHIFTED[ch]], true);
  if (ch !== lower && CHAR_TO_EVDEV[lower] !== undefined) return tap(CHAR_TO_EVDEV[lower], true);
  const code = CHAR_TO_EVDEV[ch];
  if (code === undefined) { console.warn("osk: no evdev code for", JSON.stringify(ch)); return; }
  tap(code, false);
}

// Reported back to Rust so the bar button can show which state it is in. A separate hook
// because `blur` can happen without anyone pressing the button.
W.oskState = (on) => emit({ type: "osk", on: !!on });

W.oskOpen = () => {
  const i = ensureInput();
  i.focus({ preventScroll: true });
  // iOS needs the focus to happen inside the user gesture *and* a click to commit it.
  i.click();
  W.oskState(true);
};

W.oskClose = () => {
  if (el) el.blur();
  W.oskState(false);
};

W.oskToggle = () => {
  if (el && document.activeElement === el) W.oskClose();
  else W.oskOpen();
};

// The bar's ⌨ is a `<label for="wado-osk">`, so the browser focuses this input itself, inside
// the tap. Two things still need JS:
//
//   * the input must already exist when the label is tapped — a `for=` pointing at nothing is
//     inert, and it used to be created lazily by the first `oskOpen()` that never came;
//   * a second tap should close. Label activation only ever focuses, so the close half is done
//     here, on `pointerdown` (before focus moves) and only when the field already has focus.
ensureInput();
addEventListener("pointerdown", (e) => {
  if (!e.target.closest || !e.target.closest('[for="wado-osk"]')) return;
  if (el && document.activeElement === el) { e.preventDefault(); W.oskClose(); }
}, true);
