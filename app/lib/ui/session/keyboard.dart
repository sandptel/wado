// The phone's keyboard, typing into the session (port of js/osk.js): a hidden field whose
// characters go out once each as evdev key presses, with a row of the keys a phone keyboard lacks.
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';

import '../../wado/session.dart';
import '../theme/look.dart';

const _charToEvdev = {
  'a': 30, 'b': 48, 'c': 46, 'd': 32, 'e': 18, 'f': 33, 'g': 34, 'h': 35, 'i': 23, 'j': 36, 'k': 37, 'l': 38, 'm': 50, //
  'n': 49, 'o': 24, 'p': 25, 'q': 16, 'r': 19, 's': 31, 't': 20, 'u': 22, 'v': 47, 'w': 17, 'x': 45, 'y': 21, 'z': 44, //
  '1': 2, '2': 3, '3': 4, '4': 5, '5': 6, '6': 7, '7': 8, '8': 9, '9': 10, '0': 11, //
  ' ': 57, '-': 12, '=': 13, '[': 26, ']': 27, '\\': 43, ';': 39, "'": 40, '`': 41, ',': 51, '.': 52, '/': 53, '\n': 28,
};
const _shifted = {
  '!': '1', '@': '2', '#': '3', r'$': '4', '%': '5', '^': '6', '&': '7', '*': '8', '(': '9', ')': '0', //
  '_': '-', '+': '=', '{': '[', '}': ']', '|': '\\', ':': ';', '"': "'", '~': '`', '<': ',', '>': '.', '?': '/',
};

void tapKey(int code, {bool shift = false}) {
  final s = Session.I;
  if (shift) s.sendInput({'t': 'key', 'code': 42, 'pressed': true});
  s.sendInput({'t': 'key', 'code': code, 'pressed': true});
  s.sendInput({'t': 'key', 'code': code, 'pressed': false});
  if (shift) s.sendInput({'t': 'key', 'code': 42, 'pressed': false});
}

void sendChar(String ch) {
  final lower = ch.toLowerCase();
  if (_shifted.containsKey(ch)) return tapKey(_charToEvdev[_shifted[ch]]!, shift: true);
  if (ch != lower && _charToEvdev.containsKey(lower)) return tapKey(_charToEvdev[lower]!, shift: true);
  final c = _charToEvdev[ch];
  if (c != null) tapKey(c);
}

/// The phone's keyboard, typing into the session (osk.js): a hidden field, each character sent
/// once as key presses; Enter, Backspace, Tab, Escape and arrows as their keys.
class KeyboardSheet extends StatefulWidget {
  const KeyboardSheet({super.key});
  static final _open = ValueNotifier<bool>(false);
  static void toggle(BuildContext c) => _open.value = !_open.value;
  static ValueListenable<bool> get open => _open;
  @override
  State<KeyboardSheet> createState() => _KeyboardSheetState();
}

class _KeyboardSheetState extends State<KeyboardSheet> {
  final _c = TextEditingController(text: '​');
  final _f = FocusNode();

  @override
  void initState() {
    super.initState();
    KeyboardSheet._open.addListener(_sync);
  }

  @override
  void dispose() {
    KeyboardSheet._open.removeListener(_sync);
    super.dispose();
  }

  void _sync() {
    if (KeyboardSheet._open.value) {
      _f.requestFocus();
    } else {
      _f.unfocus();
    }
    setState(() {});
  }

  void _changed(String v) {
    // A zero-width sentinel stays in the field so Backspace always has something to delete.
    if (v.isEmpty) {
      tapKey(14);
    } else {
      for (final ch in v.replaceAll('​', '').split('')) {
        sendChar(ch);
      }
    }
    _c.value = const TextEditingValue(text: '​', selection: TextSelection.collapsed(offset: 1));
  }

  @override
  Widget build(BuildContext context) {
    if (!KeyboardSheet._open.value) return const SizedBox.shrink();
    final t = Tok.of(context);
    Widget k(String label, int code) => Padding(
          padding: const EdgeInsets.all(3),
          child: Material(
            color: t.s2,
            borderRadius: BorderRadius.circular(t.r1),
            child: InkWell(
              borderRadius: BorderRadius.circular(t.r1),
              onTap: () => tapKey(code),
              child: SizedBox(height: 40, child: Center(child: Text(label, style: TextStyle(color: t.bright, fontSize: 13)))),
            ),
          ),
        );
    return Padding(
      padding: EdgeInsets.only(bottom: MediaQuery.of(context).viewInsets.bottom),
      child: Container(
        color: t.s1,
        padding: const EdgeInsets.all(4),
        child: Column(mainAxisSize: MainAxisSize.min, children: [
          Row(children: [
            Expanded(child: k('Esc', 1)),
            Expanded(child: k('Tab', 15)),
            Expanded(child: k('←', 105)),
            Expanded(child: k('↑', 103)),
            Expanded(child: k('↓', 108)),
            Expanded(child: k('→', 106)),
            Expanded(child: k('⏎', 28)),
          ]),
          SizedBox(
            height: 1,
            child: Opacity(
              opacity: 0,
              child: TextField(
                controller: _c,
                focusNode: _f,
                autocorrect: false,
                enableSuggestions: false,
                keyboardType: TextInputType.visiblePassword,
                onChanged: _changed,
                onSubmitted: (_) {
                  tapKey(28);
                  _f.requestFocus();
                },
              ),
            ),
          ),
        ]),
      ),
    );
  }
}
