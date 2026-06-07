// wado bridge — keyboard. Maps KeyboardEvent.code → Linux evdev keycode and forwards
// press/release, but only while the video is focused (so the rest of the page stays usable).
// Held keys are released on blur so nothing sticks down server-side. Unknown codes are logged.

const CODE_TO_EVDEV = {
  KeyA:30,KeyB:48,KeyC:46,KeyD:32,KeyE:18,KeyF:33,KeyG:34,KeyH:35,KeyI:23,KeyJ:36,
  KeyK:37,KeyL:38,KeyM:50,KeyN:49,KeyO:24,KeyP:25,KeyQ:16,KeyR:19,KeyS:31,KeyT:20,
  KeyU:22,KeyV:47,KeyW:17,KeyX:45,KeyY:21,KeyZ:44,
  Digit1:2,Digit2:3,Digit3:4,Digit4:5,Digit5:6,Digit6:7,Digit7:8,Digit8:9,Digit9:10,Digit0:11,
  Enter:28,Escape:1,Backspace:14,Tab:15,Space:57,
  Minus:12,Equal:13,BracketLeft:26,BracketRight:27,Backslash:43,Semicolon:39,Quote:40,
  Backquote:41,Comma:51,Period:52,Slash:53,
  ShiftLeft:42,ShiftRight:54,ControlLeft:29,ControlRight:97,AltLeft:56,AltRight:100,
  MetaLeft:125,MetaRight:126,CapsLock:58,
  ArrowUp:103,ArrowDown:108,ArrowLeft:105,ArrowRight:106,
  Home:102,End:107,PageUp:104,PageDown:109,Insert:110,Delete:111,
  F1:59,F2:60,F3:61,F4:62,F5:63,F6:64,F7:65,F8:66,F9:67,F10:68,F11:87,F12:88,
};

W.kbd = {
  down(e, video) { W.kbd._send(e, video, true); },
  up(e, video) { W.kbd._send(e, video, false); },
  _send(e, video, pressed) {
    if (document.activeElement !== video) return;
    const code = CODE_TO_EVDEV[e.code];
    if (code === undefined) { console.warn("unmapped key:", e.code); return; }
    e.preventDefault();
    if (pressed) W.pressedKeys.add(code); else W.pressedKeys.delete(code);
    W.sendInput({ t: "key", code, pressed });
  },
  releaseAll() {
    for (const code of W.pressedKeys) W.sendInput({ t: "key", code, pressed: false });
    W.pressedKeys.clear();
  },
};
