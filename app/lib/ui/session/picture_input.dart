// What a finger, a mouse and a wheel do on the picture (port of js/input_tap.js — touch as a
// translated pointer, the default — input_touch.js — raw wl_touch — input_scroll.js's two-finger
// module, input_pointer.js and gestures.js). Coordinates go through Input.norm, which reads the
// picture's content rect wherever the console or the cockpit put it, so a tap mid-morph lands
// where the finger is.
//
// Translated pointer (gestures):
//   tap                 → left click at the finger's down point
//   second tap ≤300 ms  → snapped onto the first tap's point, so the toolkit sees a double-click
//   hold still ~500 ms  → release: right click. Drag instead: left pressed where the hold began,
//                         the pointer following the finger (select, drag files)
//   one-finger drag     → finger-source scroll; the axis-stop on lift starts toolkit kinetics
//   two fingers         → whichever commits first: the gap changing is a pinch, the primary
//                         moving is a press-and-drag
//   Move windows on     → any drag is a window move
// Nothing is sent on touch-down but a hover: until the finger lifts or moves, a press would have
// to be retracted.
import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';

import '../../wado/host.dart';
import '../../wado/input.dart';
import '../../wado/settings.dart';

const _holdMs = 500, _move = 8.0, _doubleMs = 300, _doubleR = 24.0, _pinchCommit = .12;
const _swipeMin = 60.0, _edge = 14.0, _edgeSwipe = 70.0;

class _G {
  _G(this.id, this.start) : last = start;
  final int id;
  final Offset start;
  Offset last;
  String state = 'pending';
  Timer? hold;
  int? ticket, second;
  Offset? lastN, p1, p1Start, p2;
  double gap0 = 1;
  // pinch
  Offset anchor = Offset.zero;
  double d0 = 1, scale = 1, angle = 0;
  bool pinchOn = false;
  Offset? pinchN;
}

class PictureInput extends StatefulWidget {
  const PictureInput({super.key, required this.child, required this.onGesture, this.showTouches = false, this.onTouch});
  final Widget child;
  /// A recognised gesture's action (app-drawer, control-centre, back, workspace-next…).
  final ValueChanged<String> onGesture;
  final bool showTouches;
  final void Function(int id, Offset at, String phase)? onTouch;
  @override
  State<PictureInput> createState() => _PictureInputState();
}

class _PictureInputState extends State<PictureInput> {
  final _active = <int>{};
  _G? g;
  ({DateTime t, Offset at, Offset n})? _lastTap;
  // gestures.js
  ({int id, Offset at, String side})? _edgeG;
  bool _swipe3 = false;
  final _pts3 = <int, (Offset, Offset)>{};
  bool _mouseDrag = false;
  final _focus = FocusNode();

  Input get I => Input.I;
  bool get _raw => Settings.I.s('touch_mode') == 'touch';
  bool get _moveMode => Settings.I.b('move_mode');

  @override
  void dispose() {
    g?.hold?.cancel();
    _focus.dispose();
    super.dispose();
  }

  // ── gestures.js ──
  bool _edgeDown(PointerDownEvent e) {
    if (!Settings.I.b('edge_swipe') || _active.length != 1) return false;
    final w = MediaQuery.sizeOf(context).width;
    final left = e.position.dx < _edge, right = e.position.dx > w - _edge;
    if (!left && !right) return false;
    _edgeG = (id: e.pointer, at: e.position, side: left ? 'left' : 'right');
    return true;
  }

  bool _edgeUp(PointerEvent e) {
    final eg = _edgeG;
    if (eg == null || eg.id != e.pointer) return false;
    _edgeG = null;
    final d = e.position - eg.at;
    if (d.dx.abs() > _edgeSwipe && d.dx.abs() > d.dy.abs() * 1.5) widget.onGesture(d.dx > 0 ? 'workspace-prev' : 'workspace-next');
    return true;
  }

  bool _swipeDown(PointerDownEvent e) {
    if (_swipe3) {
      _pts3[e.pointer] = (e.position, e.position);
      return true;
    }
    if (_active.length != 3 || _raw) return false;
    if (g != null && g!.state != 'pending' && g!.state != 'two') return false;
    g?.hold?.cancel();
    g = null;
    _swipe3 = true;
    _pts3
      ..clear()
      ..[e.pointer] = (e.position, e.position);
    return true;
  }

  bool _swipeUp() {
    if (!_swipe3) return false;
    if (_active.isNotEmpty) return true; // wait for the last finger
    _swipe3 = false;
    var d = Offset.zero;
    for (final (a, b) in _pts3.values) {
      d += b - a;
    }
    d /= math.max(1, _pts3.length).toDouble();
    if (math.max(d.dx.abs(), d.dy.abs()) < _swipeMin) return true;
    final dir = d.dx.abs() > d.dy.abs() ? (d.dx > 0 ? 'right' : 'left') : (d.dy > 0 ? 'down' : 'up');
    final custom = Host.I.config.value?['gestures'];
    const dflt = {'up': 'app-drawer', 'down': 'control-centre', 'left': 'back', 'right': 'focus-next'};
    final action = custom is Map && custom['swipe-3-$dir'] != null ? '${custom['swipe-3-$dir']}' : dflt[dir]!;
    if (action != 'none') widget.onGesture(action);
    return true;
  }

  // ── window drag (Move windows) ──
  void _drag(String phase, Offset at) {
    final n = I.norm(at);
    if (n == null) return;
    final ev = {'t': 'window_drag', 'phase': phase, 'x': n.dx, 'y': n.dy};
    phase == 'motion' ? I.queue('window_drag', ev) : I.now(ev);
  }

  void _motion(Offset at) {
    final n = I.norm(at);
    if (n != null) I.queue('pointer_motion', {'t': 'pointer_motion', 'x': n.dx, 'y': n.dy});
  }

  // ── the two-finger module (input_scroll.js scrollg) ──
  bool _pinchBegin(int second, Offset at) {
    final gg = g;
    if (gg == null) return false;
    if (gg.state == 'scroll') return true;
    gg.hold?.cancel();
    if (gg.state == 'tap' || gg.state == 'touch') {
      I.now({'t': 'cancel_touch', 'id': gg.id & 0xffffffff});
    } else if (gg.state == 'move') {
      _drag('up', at);
    }
    gg.state = 'scroll';
    gg.second = second;
    gg.p1 = gg.last;
    gg.p2 = at;
    gg.anchor = (gg.p1! + gg.p2!) / 2;
    gg.d0 = math.max(1, (gg.p2! - gg.p1!).distance);
    gg.scale = 1;
    gg.angle = math.atan2(gg.p2!.dy - gg.p1!.dy, gg.p2!.dx - gg.p1!.dx);
    final pn = I.norm(gg.anchor);
    if (pn != null) {
      gg.pinchOn = true;
      gg.pinchN = pn;
      I.send({'t': 'pinch', 'phase': 'down', 'x': pn.dx, 'y': pn.dy, 'scale': 1, 'rotation': 0});
    }
    return true;
  }

  bool _pinchMove(PointerMoveEvent e) {
    final gg = g;
    if (gg == null || gg.state != 'scroll') return false;
    if (e.pointer == gg.id) {
      gg.p1 = e.position;
    } else if (e.pointer == gg.second) {
      gg.p2 = e.position;
    } else {
      return false;
    }
    final m = (gg.p1! + gg.p2!) / 2;
    if (gg.pinchOn) {
      final scale = (gg.p2! - gg.p1!).distance / gg.d0;
      final angle = math.atan2(gg.p2!.dy - gg.p1!.dy, gg.p2!.dx - gg.p1!.dx);
      var rot = (angle - gg.angle) * 180 / math.pi;
      rot -= 360 * (rot / 360).round();
      if ((scale - gg.scale).abs() > .01 || rot.abs() > .5) {
        final pn = I.norm(m);
        if (pn != null) {
          I.send({'t': 'pinch', 'phase': 'motion', 'x': pn.dx, 'y': pn.dy, 'scale': scale, 'rotation': rot});
          gg.scale = scale;
          gg.angle = angle;
          gg.pinchN = pn;
        }
      }
    }
    final d = m - gg.anchor;
    if (d.distance < 2) return true;
    gg.anchor = m;
    gg.lastN = I.scrollBy(d.dx, d.dy, m) ?? gg.lastN;
    return true;
  }

  bool _pinchEnd(PointerEvent e) {
    final gg = g;
    if (gg == null || gg.state != 'scroll') return false;
    if (e.pointer != gg.id && e.pointer != gg.second) return false;
    if (gg.pinchOn && gg.pinchN != null) {
      I.send({'t': 'pinch', 'phase': 'up', 'x': gg.pinchN!.dx, 'y': gg.pinchN!.dy, 'scale': gg.scale, 'rotation': 0});
    }
    I.scrollStop(gg.lastN);
    g = null;
    return true;
  }

  // ── touch as a translated pointer (input_tap.js) ──
  void _tapDown(PointerDownEvent e) {
    final gg = g;
    if (gg == null) {
      final ng = _G(e.pointer, e.position);
      if (_moveMode) {
        ng.state = 'move';
        _drag('down', e.position);
      } else {
        ng.hold = Timer(const Duration(milliseconds: _holdMs), () {
          if (g == ng && ng.state == 'pending') ng.state = 'held';
        });
        _motion(e.position); // hover only: never needs retracting
        ng.ticket = I.ask(e.position);
      }
      g = ng;
      return;
    }
    if (gg.state == 'scroll' || gg.second != null || gg.state == 'hold-drag') return;
    gg.hold?.cancel();
    if (gg.state == 'pan') I.scrollStop(gg.lastN);
    if (gg.state == 'move') _drag('up', gg.last);
    gg.state = 'two';
    gg.second = e.pointer;
    gg.p1 = gg.last;
    gg.p1Start = gg.last;
    gg.p2 = e.position;
    gg.gap0 = math.max(1, (gg.p2! - gg.p1!).distance);
  }

  void _tapMove(PointerMoveEvent e) {
    if (_pinchMove(e)) return;
    final gg = g;
    if (gg == null) return;
    if (gg.state == 'two' || gg.state == 'drag2') {
      if (e.pointer == gg.id) {
        gg.p1 = e.position;
      } else if (e.pointer == gg.second) {
        gg.p2 = e.position;
      } else {
        return;
      }
      if (gg.state == 'drag2') {
        if (e.pointer == gg.id) _motion(e.position);
        return;
      }
      final gap = (gg.p2! - gg.p1!).distance;
      if ((gap / gg.gap0 - 1).abs() > _pinchCommit) {
        gg.state = 'pinch-handover';
        gg.last = gg.p1!;
        _pinchBegin(gg.second!, gg.p2!);
      } else if ((gg.p1! - gg.p1Start!).distance > _move) {
        final n0 = I.norm(gg.p1Start!);
        if (n0 == null) return;
        gg.state = 'drag2';
        I.now({'t': 'button', 'x': n0.dx, 'y': n0.dy, 'button': 'left', 'pressed': true});
        _motion(gg.p1!);
      }
      return;
    }
    if (e.pointer != gg.id) return;
    final d = e.position - gg.last;
    final dist = (e.position - gg.start).distance;
    switch (gg.state) {
      case 'pending':
        if (dist <= _move) return;
        gg.hold?.cancel();
        gg.state = 'pan';
        // From the down point: the threshold's worth of travel is scrolled, not lost.
        final from = e.position - gg.start;
        gg.lastN = I.scrollBy(from.dx, from.dy, e.position) ?? gg.lastN;
      case 'pan':
        gg.lastN = I.scrollBy(d.dx, d.dy, e.position) ?? gg.lastN;
      case 'held':
        if (dist <= _move) return;
        final n0 = I.norm(gg.start);
        if (n0 == null) return;
        gg.state = 'hold-drag';
        I.now({'t': 'button', 'x': n0.dx, 'y': n0.dy, 'button': 'left', 'pressed': true});
        _motion(e.position);
      case 'hold-drag':
        _motion(e.position);
      case 'move':
        _drag('motion', e.position);
    }
    gg.last = e.position;
  }

  void _tapUp(PointerEvent e) {
    if (_pinchEnd(e)) return;
    final gg = g;
    if (gg == null) return;
    if (gg.state == 'two' || gg.state == 'drag2') {
      if (e.pointer != gg.id && e.pointer != gg.second) return;
      if (gg.state == 'drag2') {
        final n = I.norm(gg.p1!);
        if (n != null) I.now({'t': 'button', 'x': n.dx, 'y': n.dy, 'button': 'left', 'pressed': false});
      }
      g = null;
      return;
    }
    if (e.pointer != gg.id) return;
    gg.hold?.cancel();
    switch (gg.state) {
      case 'pending':
        if (e is PointerUpEvent) _tap(gg);
      case 'pan':
        I.scrollStop(gg.lastN);
      case 'held':
        final n = I.norm(gg.start);
        if (n != null) I.click(n, 'right');
      case 'hold-drag':
        final n = I.norm(e.position);
        if (n != null) I.now({'t': 'button', 'x': n.dx, 'y': n.dy, 'button': 'left', 'pressed': false});
      case 'move':
        _drag('up', e.position);
    }
    g = null;
  }

  void _tap(_G gg) {
    final now = DateTime.now();
    final prev = _lastTap;
    final snapped = prev != null && now.difference(prev.t).inMilliseconds <= _doubleMs && (gg.start - prev.at).distance <= _doubleR;
    final n = snapped ? prev.n : I.norm(gg.start);
    if (n == null) return;
    // Chained from the first tap's point, so a triple-tap stays on the same spot too.
    _lastTap = (t: now, at: snapped ? prev.at : gg.start, n: n);
    // The second tap of a double is already aimed — at the first. Only a fresh tap consults the
    // targets under the finger (asked on touch-down; never waits more than 70 ms past lift).
    if (snapped || gg.ticket == null) return I.click(n, 'left');
    I.wait(gg.ticket!).then((targets) => I.click(Input.decide(n, targets), 'left'));
  }

  // ── raw wl_touch (input_touch.js) ──
  void _touchAt(int id, String phase, Offset at) {
    final n = I.norm(at);
    if (n == null) return;
    final ev = {'t': 'touch', 'id': id & 0xffffffff, 'phase': phase, 'x': n.dx, 'y': n.dy};
    phase == 'motion' ? I.queue('touch:$id', ev) : I.now(ev);
  }

  void _rawDown(PointerDownEvent e) {
    if (g == null) {
      final ng = _G(e.pointer, e.position);
      if (_moveMode) {
        ng.state = 'move';
        _drag('down', e.position);
      } else {
        ng.state = 'tap';
        _touchAt(e.pointer, 'down', e.position);
        ng.hold = Timer(const Duration(milliseconds: _holdMs), () {
          if (g != ng || ng.state != 'tap') return;
          ng.state = 'held';
          I.send({'t': 'cancel_touch', 'id': ng.id & 0xffffffff});
        });
      }
      g = ng;
    } else if (!_pinchBegin(e.pointer, e.position)) {
      _touchAt(e.pointer, 'down', e.position);
    }
  }

  void _rawMove(PointerMoveEvent e) {
    if (_pinchMove(e)) return;
    final gg = g;
    if (gg != null && e.pointer == gg.id) {
      gg.last = e.position;
      final dist = (e.position - gg.start).distance;
      switch (gg.state) {
        case 'tap':
          if (dist > _move) {
            gg.hold?.cancel();
            gg.state = 'touch';
            _touchAt(gg.id, 'motion', e.position);
          }
        case 'touch':
          _touchAt(gg.id, 'motion', e.position);
        case 'held':
          if (dist > _move) {
            gg.state = 'move';
            _drag('down', gg.start);
            _drag('motion', e.position);
          }
        case 'move':
          _drag('motion', e.position);
      }
    } else {
      _touchAt(e.pointer, 'motion', e.position);
    }
  }

  void _rawUp(PointerEvent e) {
    if (_pinchEnd(e)) return;
    final gg = g;
    if (gg != null && e.pointer == gg.id) {
      gg.hold?.cancel();
      switch (gg.state) {
        case 'tap' || 'touch':
          _touchAt(gg.id, 'up', e.position);
        case 'move':
          _drag('up', e.position);
        case 'held':
          final n = I.norm(e.position);
          if (n != null) I.click(n, 'right');
      }
      g = null;
    } else {
      _touchAt(e.pointer, 'up', e.position);
    }
  }

  // ── a mouse (input_pointer.js) ──
  static String? _btn(int b) => b & kPrimaryButton != 0 ? 'left' : (b & kSecondaryButton != 0 ? 'right' : (b & kMiddleMouseButton != 0 ? 'middle' : null));
  int _held = 0;

  void _mouseDown(PointerDownEvent e) {
    final n = I.norm(e.position);
    if (n == null) return;
    if (_moveMode && e.buttons & kPrimaryButton != 0) {
      _mouseDrag = true;
      I.now({'t': 'window_drag', 'phase': 'down', 'x': n.dx, 'y': n.dy});
      return;
    }
    final b = _btn(e.buttons);
    if (b == null) return;
    _held = e.buttons;
    I.now({'t': 'button', 'x': n.dx, 'y': n.dy, 'button': b, 'pressed': true});
  }

  void _mouseUp(PointerEvent e) {
    final n = I.norm(e.position);
    if (n == null) return;
    if (_mouseDrag) {
      _mouseDrag = false;
      I.now({'t': 'window_drag', 'phase': 'up', 'x': n.dx, 'y': n.dy});
      return;
    }
    final b = _btn(_held);
    _held = 0;
    if (b != null) I.now({'t': 'button', 'x': n.dx, 'y': n.dy, 'button': b, 'pressed': false});
  }

  void _wheel(PointerScrollEvent e) {
    final n = I.norm(e.position);
    if (n == null) return;
    final sign = Settings.I.b('natural_scroll') ? -1 : 1;
    final speed = Settings.I.d('scroll_speed') * I.gain(e.scrollDelta.distance) * I.cssToLogical;
    I.send({'t': 'scroll', 'x': n.dx, 'y': n.dy, 'dx': e.scrollDelta.dx * speed * sign, 'dy': e.scrollDelta.dy * speed * sign});
  }

  @override
  Widget build(BuildContext context) {
    bool mouse(PointerEvent e) => e.kind == PointerDeviceKind.mouse;
    return Focus(
      focusNode: _focus,
      autofocus: true,
      onKeyEvent: (_, e) => I.key(e) ? KeyEventResult.handled : KeyEventResult.ignored,
      onFocusChange: (f) {
        if (!f) I.releaseKeys();
      },
      child: Listener(
        behavior: HitTestBehavior.opaque,
        onPointerDown: (e) {
          _focus.requestFocus();
          _active.add(e.pointer);
          widget.onTouch?.call(e.pointer, e.position, 'down');
          if (mouse(e)) return _mouseDown(e);
          if (_edgeDown(e) || _swipeDown(e)) return;
          _raw ? _rawDown(e) : _tapDown(e);
        },
        onPointerMove: (e) {
          if (mouse(e)) {
            final n = I.norm(e.position);
            if (n == null) return;
            if (_mouseDrag) return I.queue('window_drag', {'t': 'window_drag', 'phase': 'motion', 'x': n.dx, 'y': n.dy});
            return I.queue('pointer_motion', {'t': 'pointer_motion', 'x': n.dx, 'y': n.dy});
          }
          if (!_active.contains(e.pointer)) return;
          widget.onTouch?.call(e.pointer, e.position, 'move');
          if (_edgeG?.id == e.pointer) return;
          if (_swipe3) {
            final p = _pts3[e.pointer];
            if (p != null) _pts3[e.pointer] = (p.$1, e.position);
            return;
          }
          _raw ? _rawMove(e) : _tapMove(e);
        },
        onPointerHover: (e) => _motion(e.position),
        onPointerUp: (e) => _end(e),
        onPointerCancel: (e) => _end(e),
        onPointerSignal: (e) {
          if (e is PointerScrollEvent) _wheel(e);
        },
        child: widget.child,
      ),
    );
  }

  void _end(PointerEvent e) {
    final had = _active.remove(e.pointer);
    widget.onTouch?.call(e.pointer, e.position, 'up');
    if (e.kind == PointerDeviceKind.mouse) return _mouseUp(e);
    if (!had) return;
    if (_edgeUp(e) || _swipeUp()) return;
    _raw ? _rawUp(e) : _tapUp(e);
  }
}
