// Touch on the picture, as a translated pointer (port of js/input_tap.js, the default "pointer"
// touch mode) and the on-screen keyboard (js/osk.js). Coordinates are normalised 0..1 to the
// displayed video content, letterboxing removed (input_core.js `normPoint`).
//
//   tap                 → left click at the finger's down point
//   second tap ≤300 ms  → left click snapped onto the first tap's point (a double-click)
//   hold still ~500 ms  → lift: right click; drag instead: press, follow, release (drag-and-drop)
//   one-finger drag     → finger-source scroll; an axis stop on lift
import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../../wado/session.dart';
import '../../wado/settings.dart';
import '../theme/look.dart';

const _hold = Duration(milliseconds: 500);
const _moveThreshold = 8.0;
const _doubleMs = 300;
const _doublePx = 24.0;

class InputLayer extends StatefulWidget {
  const InputLayer({super.key, required this.child});
  final Widget child;
  @override
  State<InputLayer> createState() => _InputLayerState();
}

enum _G { tap, scroll, held, drag }

class _InputLayerState extends State<InputLayer> {
  int? _id;
  Offset _start = Offset.zero, _last = Offset.zero;
  _G _state = _G.tap;
  Timer? _holdTimer;
  DateTime _lastTap = DateTime.fromMillisecondsSinceEpoch(0);
  Offset? _lastTapAt;
  Size _box = Size.zero;

  /// The picture's rect inside the box (object-fit: contain).
  Offset? _norm(Offset p) {
    final r = Session.I.renderer;
    final vw = r.videoWidth.toDouble(), vh = r.videoHeight.toDouble();
    if (vw <= 0 || vh <= 0 || _box.isEmpty) return null;
    final s = math.min(_box.width / vw, _box.height / vh);
    final cw = vw * s, ch = vh * s;
    final ox = (_box.width - cw) / 2, oy = (_box.height - ch) / 2;
    return Offset(((p.dx - ox) / cw).clamp(0, 1), ((p.dy - oy) / ch).clamp(0, 1));
  }

  /// CSS px → the output's logical px (input_units.js `cssToLogical`).
  double _toLogical() {
    final r = Session.I.renderer;
    final vw = r.videoWidth.toDouble(), vh = r.videoHeight.toDouble();
    if (vw <= 0 || vh <= 0 || _box.isEmpty) return 1;
    final fit = math.min(_box.width / vw, _box.height / vh);
    final scale = (Session.config()['scale'] as num).toDouble();
    return 1 / (fit * (scale > 0 ? scale : 1));
  }

  void _send(Map<String, dynamic> m) => Session.I.sendInput(m);

  void _motion(Offset p) {
    final n = _norm(p);
    if (n != null) _send({'t': 'pointer_motion', 'x': n.dx, 'y': n.dy});
  }

  void _button(Offset p, String b, bool pressed) {
    final n = _norm(p);
    if (n != null) _send({'t': 'button', 'x': n.dx, 'y': n.dy, 'button': b, 'pressed': pressed});
  }

  void _click(Offset p, String b) {
    _motion(p);
    _button(p, b, true);
    _button(p, b, false);
  }

  bool get _raw => Settings.I.s('touch_mode') == 'touch';

  /// Raw touch (input_touch.js): each finger reaches the app as wl_touch.
  void _touch(PointerEvent e, String phase) {
    final n = _norm(e.localPosition);
    if (n != null) _send({'t': 'touch', 'id': e.pointer & 0xffffffff, 'phase': phase, 'x': n.dx, 'y': n.dy});
  }

  void _down(PointerDownEvent e) {
    if (_raw) return _touch(e, 'down');
    if (_id != null) return;
    _id = e.pointer;
    _start = _last = e.localPosition;
    _state = _G.tap;
    _motion(e.localPosition);
    _holdTimer = Timer(_hold, () {
      if (_state == _G.tap) {
        _state = _G.held;
        HapticFeedback.mediumImpact();
      }
    });
  }

  void _move(PointerMoveEvent e) {
    if (_raw) return _touch(e, 'motion');
    if (e.pointer != _id) return;
    final p = e.localPosition;
    final dist = (p - _start).distance;
    switch (_state) {
      case _G.tap:
        if (dist > _moveThreshold) {
          _holdTimer?.cancel();
          _state = _G.scroll;
          _scroll(p - _last, p);
        }
      case _G.scroll:
        _scroll(p - _last, p);
      case _G.held:
        if (dist > _moveThreshold) {
          _state = _G.drag;
          _motion(_start);
          _button(_start, 'left', true);
          _motion(p);
        }
      case _G.drag:
        _motion(p);
    }
    _last = p;
  }

  void _scroll(Offset d, Offset at) {
    final n = _norm(at);
    if (n == null) return;
    final s = Settings.I;
    final sign = s.b('natural_scroll') ? -1 : 1;
    final speed = s.d('scroll_speed') * _toLogical() * 3;
    _send({'t': 'scroll', 'x': n.dx, 'y': n.dy, 'dx': -d.dx * speed * sign, 'dy': -d.dy * speed * sign, 'source': 'finger'});
  }

  void _up(PointerEvent e) {
    if (_raw) return _touch(e, 'up');
    if (e.pointer != _id) return;
    _holdTimer?.cancel();
    final p = e.localPosition;
    switch (_state) {
      case _G.tap:
        final now = DateTime.now();
        var at = _start;
        if (_lastTapAt != null && now.difference(_lastTap).inMilliseconds <= _doubleMs && (at - _lastTapAt!).distance <= _doublePx) {
          at = _lastTapAt!;
        }
        _click(at, 'left');
        _lastTap = now;
        _lastTapAt = at;
      case _G.scroll:
        final n = _norm(p);
        if (n != null) _send({'t': 'scroll', 'x': n.dx, 'y': n.dy, 'dx': 0, 'dy': 0, 'source': 'finger', 'stop': true});
      case _G.held:
        _click(_start, 'right');
      case _G.drag:
        _button(p, 'left', false);
    }
    _id = null;
  }

  @override
  Widget build(BuildContext context) {
    return LayoutBuilder(builder: (context, box) {
      _box = box.biggest;
      return Listener(
        behavior: HitTestBehavior.opaque,
        onPointerDown: _down,
        onPointerMove: _move,
        onPointerUp: _up,
        onPointerCancel: _up,
        onPointerSignal: (s) {
          if (s is PointerScrollEvent) {
            final n = _norm(s.localPosition);
            if (n != null) _send({'t': 'scroll', 'x': n.dx, 'y': n.dy, 'dx': s.scrollDelta.dx, 'dy': s.scrollDelta.dy, 'source': 'wheel'});
          }
        },
        child: Stack(children: [Positioned.fill(child: widget.child), const Positioned(left: 0, right: 0, bottom: 0, child: KeyboardSheet())]),
      );
    });
  }
}

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
