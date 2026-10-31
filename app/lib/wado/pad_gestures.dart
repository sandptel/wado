// Phone gestures (port of js/pad_gestures.js): the few motions a laptop touchpad never makes,
// claimed for an action the user picked. Every pointer asks `pass` first, and a sequence is held
// back or swallowed only by these rules:
//
//   edge-left / edge-right  one finger landing within 10 px of that edge and moving inward far
//                           enough within 300 ms. Only that finger is held, for at most 60 ms:
//                           then it is claimed, or replayed to the pad as the touch it was.
//   four-tap                four fingers down and up within 250 ms, moving < 12 px. The fourth is
//                           not forwarded and the three on the computer are held still; a
//                           four-finger swipe after all hands them back. The lift waits past
//                           libinput's tap timeout, so no three-finger tap fires.
//   two-hold                two fingers still (< 8 px) long enough. They were forwarded all along
//                           (two resting fingers do nothing); on the claim they are lifted on the
//                           computer, then the action runs.
//
// A gesture with no action — or one this computer does not offer — is never claimed and nothing
// is held. Two- and three-finger swipes and pinches are never claimed.
import 'dart:async';
import 'dart:ui' show Offset, Size;

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart' show HapticFeedback;

import 'host.dart';
import 'pad.dart';
import 'pad_actions.dart';

class _F {
  _F(this.at, this.size) : from = at;
  final Offset from;
  Offset at;
  Size size;
  double get moved => (at - from).distance;
}

class PadGestures {
  PadGestures._();
  static final I = PadGestures._();

  static const _edgePx = 10.0, _edgeMs = 300, _holdBackMs = 60, _edgeLean = 6.0;
  static const _fourMs = 250, _fourStill = 12.0, _tapLiftMs = 200, _twoStill = 8.0;
  /// wado's own gestures follow "How far a gesture goes"; the computer's are its own.
  static const _distance = {'short': (28.0, 450), 'medium': (40.0, 600), 'long': (56.0, 800)};

  /// The edge being swiped from: side, finger's y, progress 0..1 (and past 1 once fired).
  final edgeGlow = ValueNotifier<({String side, double y, double p})?>(null);
  /// A claim happened: the action's icon, where.
  final fired = StreamController<({String icon, Offset at})>.broadcast();

  final _fingers = <int, _F>{};
  final _sw = Stopwatch()..start();
  /// null · edge-hold · edge · four · two — the claim on this sequence.
  String? _mode;
  int _start = 0;
  ({String side, int id, List<(String, Offset, Size)> held, Map<String, dynamic> action, Timer timer})? _edge;
  bool _edgeFired = false;
  ({int fourth, List<int> frozen, Map<String, dynamic> action, Offset at})? _four;
  Timer? _two;

  int get _now => _sw.elapsedMilliseconds;

  Map get _prefs => (Host.I.config.value?['trackpad'] as Map?) ?? const {};
  (double, int) get _dist => _distance['${_prefs['gesture_distance'] ?? 'medium'}'] ?? _distance['medium']!;

  Map<String, dynamic>? _action(String name) {
    final id = (_prefs['gestures'] as Map?)?[name];
    return id is String && id.isNotEmpty ? PadActions.I.byId(id) : null;
  }

  void _fire(Map<String, dynamic> action, Offset at) {
    HapticFeedback.mediumImpact();
    fired.add((icon: '${action['icon'] ?? 'bolt'}', at: at));
    PadActions.I.run('${action['id']}');
  }

  // ── edge ──
  double _inward(_F f) => _edge!.side == 'edge-left' ? f.at.dx - f.from.dx : f.from.dx - f.at.dx;

  void _edgeRelease() {
    // Not an edge swipe: the held touches go to the pad as they happened.
    final e = _edge!;
    e.timer.cancel();
    _edge = null;
    _mode = null;
    edgeGlow.value = null;
    final down = e.held.where((h) => h.$1 == 'down').firstOrNull;
    final last = e.held.lastWhere((h) => h.$1 == 'move', orElse: () => ('', Offset.zero, Size.zero));
    if (down != null) PadTouch.I.down(e.id, down.$2.dx, down.$2.dy, down.$3.width, down.$3.height);
    if (last.$1.isNotEmpty) scheduleMicrotask(() => PadTouch.I.move(e.id, last.$2.dx, last.$2.dy, last.$3.width, last.$3.height));
  }

  void _edgeCheck(_F f) {
    final e = _edge!;
    final need = _dist.$1;
    edgeGlow.value = (side: e.side, y: f.at.dy, p: (_inward(f) / need).clamp(0, _edgeFired ? 2 : 1));
    if (!_edgeFired && _inward(f) >= need && _now - _start <= _edgeMs) {
      _edgeFired = true;
      if (_mode == 'edge-hold') {
        e.timer.cancel();
        _mode = 'edge';
      }
      _fire(e.action, f.at);
    }
  }

  void _edgeDecide() {
    if (_mode != 'edge-hold' || _edge == null) return;
    final f = _fingers[_edge!.id];
    final lean = f == null ? 0.0 : _inward(f);
    if (f != null && lean >= _edgeLean && lean > (f.at.dy - f.from.dy).abs()) {
      _mode = 'edge';
    } else {
      _edgeRelease();
    }
  }

  // ── four ──
  void _fourHandBack() {
    // A four-finger swipe after all: the held fingers move to where they are, the fourth lands.
    final f4 = _four!;
    _four = null;
    _mode = null;
    PadTouch.I.drop(f4.frozen.where((id) => !_fingers.containsKey(id)));
    for (final id in f4.frozen) {
      final f = _fingers[id];
      if (f != null) PadTouch.I.move(id, f.at.dx, f.at.dy, f.size.width, f.size.height);
    }
    final f = _fingers[f4.fourth];
    if (f != null) PadTouch.I.down(f4.fourth, f.at.dx, f.at.dy, f.size.width, f.size.height);
  }

  void _fourEnd() {
    final f4 = _four!;
    _four = null;
    final tap = _now - _start <= _fourMs;
    final wait = _start + _tapLiftMs - _now;
    if (wait > 0) {
      Timer(Duration(milliseconds: wait), () => PadTouch.I.drop(f4.frozen));
    } else {
      PadTouch.I.drop(f4.frozen);
    }
    if (tap) _fire(f4.action, f4.at);
  }

  // ── two ──
  void _twoCancel() {
    _two?.cancel();
    _two = null;
  }

  void _twoFire() {
    _two = null;
    final action = _action('two-hold');
    if (action == null || _fingers.length != 2 || _mode != null) return;
    final fs = _fingers.values.toList();
    if (fs.any((f) => f.moved >= _twoStill)) return;
    _mode = 'two';
    PadTouch.I.drop(_fingers.keys.toList()); // lift them on the computer first
    _fire(action, (fs[0].at + fs[1].at) / 2);
  }

  // ── the gate: true lets PadTouch have it ──
  bool down(int id, Offset p, Size s) {
    final first = _fingers.isEmpty;
    if (first) {
      _start = _now;
      _mode = null;
    }
    _fingers[id] = _F(p, s);
    if (_mode == 'edge-hold') {
      _edgeRelease(); // a second finger: no edge swipe
      return true;
    }
    if (_mode != null) return false; // claimed: later fingers are part of it
    if (first) {
      final side = p.dx < _edgePx ? 'edge-left' : (p.dx > s.width - _edgePx ? 'edge-right' : null);
      final action = side == null ? null : _action(side);
      if (side != null && action != null) {
        _mode = 'edge-hold';
        _edgeFired = false;
        _edge = (side: side, id: id, held: [('down', p, s)], action: action, timer: Timer(const Duration(milliseconds: _holdBackMs), _edgeDecide));
        return false;
      }
    }
    if (_fingers.length == 4 && _now - _start <= _fourMs) {
      final action = _action('four-tap');
      if (action != null && _fingers.values.every((g) => g.moved < _fourStill)) {
        _mode = 'four';
        final at = _fingers.values.fold(Offset.zero, (a, g) => a + g.at) / 4;
        _four = (fourth: id, frozen: [for (final k in _fingers.keys) if (k != id) k], action: action, at: at);
        _twoCancel();
        return false;
      }
    }
    _twoCancel();
    if (_fingers.length == 2 && _action('two-hold') != null) _two = Timer(Duration(milliseconds: _dist.$2), _twoFire);
    return true;
  }

  bool move(int id, Offset p, Size s) {
    final f = _fingers[id];
    if (f == null) return true;
    f
      ..at = p
      ..size = s;
    switch (_mode) {
      case 'edge-hold':
        if (id == _edge!.id) {
          _edge!.held.add(('move', p, s));
          _edgeCheck(f);
        }
        return false;
      case 'edge':
        if (_edge != null && id == _edge!.id) _edgeCheck(f);
        return false;
      case 'four':
        if (f.moved >= _fourStill) {
          _fourHandBack();
          return true;
        }
        return false;
      case null:
        if (_two != null && f.moved >= _twoStill) _twoCancel(); // a scroll, not a hold
        return true;
      default:
        return false;
    }
  }

  bool up(int id) {
    if (!_fingers.containsKey(id)) return true;
    final was = _mode;
    var pass = true;
    if (was == 'edge-hold') {
      if (id == _edge!.id) _edgeRelease(); // a tap at the edge: replayed, and this up follows it
    } else if (was != null) {
      pass = false;
    } else {
      _twoCancel();
    }
    _fingers.remove(id);
    if (_fingers.isEmpty) {
      if (was == 'four' && _four != null) _fourEnd();
      _edge?.timer.cancel();
      _edge = null;
      edgeGlow.value = null;
      _twoCancel();
      _mode = null;
    }
    return pass;
  }

  void reset() {
    _edge?.timer.cancel();
    _twoCancel();
    _fingers.clear();
    _edge = null;
    _four = null;
    _mode = null;
    edgeGlow.value = null;
  }
}
