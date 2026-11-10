// Phone gestures (port of js/pad_gestures.js): the few motions a laptop touchpad never makes,
// claimed for an action the user picked. Every pointer asks `pass` first, and a sequence is held
// back or swallowed only by these rules:
//
//   edge-left / edge-right  one finger landing within 10 px of that edge and moving inward far
//                           enough within 300 ms. Only that finger is held, for at most 60 ms:
//                           then it is claimed, or replayed to the pad as the touch it was.
//   edge scrolling          one finger landing in the edge zone (`edge-side`, `edge-width`) and
//                           moving along it scrolls. On a computer that takes `PadMsg::Scroll`
//                           (`pad-scroll`) it is a scroll stick: where it landed is the centre,
//                           and held away from it, it keeps scrolling — faster the further,
//                           `edge-dir` choosing which way up goes — until let go. Elsewhere it is
//                           sent as two fingers (it and a ghost) moving with the finger. Held
//                           until it moves: along the edge it scrolls; any other way, or a tap,
//                           it is replayed to the pad as it happened.
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
import 'dart:math' as math;
import 'dart:ui' show Offset, Size;

import 'package:flutter/foundation.dart';

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
  /// Edge scrolling under way: which side, where the finger landed (the stick's centre), where it
  /// is now, and how fast it scrolls (0..1).
  final edgeScroll = ValueNotifier<({String side, double anchor, double y, double speed})?>(null);
  /// What edge scrolling asks the hand to feel (`padFeels` names): a tick as it takes hold or
  /// speeds up a step, a soft notch per stretch scrolled, faster as it goes.
  final scrollFelt = StreamController<String>.broadcast();

  final _fingers = <int, _F>{};
  final _sw = Stopwatch()..start();
  /// null · edge-hold · edge · four · two — the claim on this sequence.
  String? _mode;
  int _start = 0;
  ({String side, int id, List<(String, Offset, Size)> held, Map<String, dynamic>? action, String? scroll, Timer? timer})? _edge;
  /// The edge scroll: its finger, which side, where it landed (the centre), and — sent as two
  /// fingers to a computer without `pad-scroll` — the ghost beside it and the x both keep.
  ({int id, int ghost, String side, double x, double gx, double anchor, Size size, bool stick})? _es;
  double _notch = 0, _lastY = 0, _off = 0;
  int _band = 0, _lastFelt = 0, _lastTick = 0;
  Timer? _stick;
  static const _ghostGap = 48.0, _scrollLean = 6.0, _notchPx = 36.0, _ghostBase = 1 << 20;
  /// The stick: no scroll this close to the centre; a notch felt per this much scrolled.
  static const stickDead = 8.0, _stickNotchPx = 60.0, _stickFeelMs = 45;
  bool _edgeFired = false;
  ({int fourth, List<int> frozen, Map<String, dynamic> action, Offset at})? _four;
  Timer? _two;

  int get _now => _sw.elapsedMilliseconds;

  Map get _prefs => (Host.I.config.value?['trackpad'] as Map?) ?? const {};

  /// The edge zone `p` lands in, if edge scrolling is on and it does: `left` or `right`.
  String? _scrollSide(Offset p, Size s) {
    if (_prefs['edge_scroll'] == false) return null;
    final side = '${_prefs['edge_side'] ?? 'right'}';
    final w = ((_prefs['edge_width'] as num?) ?? 32).toDouble();
    if (side != 'left' && p.dx > s.width - w) return 'right';
    if (side != 'right' && p.dx < w) return 'left';
    return null;
  }

  // ── edge scrolling ──
  /// How fast the stick scrolls, in pad px per second as a finger would move, `off` px from its
  /// centre: nothing in the dead zone, then gently, then quickly — a curve, so a small push is
  /// fine control and a long one crosses a page.
  static double stickSpeed(double off) {
    final a = off.abs() - stickDead;
    if (a <= 0) return 0;
    final t = (a / 90).clamp(0.0, 1.8);
    return off.sign * math.pow(t, 1.5) * 1400;
  }

  /// 0 still · 1 slow · 2 brisk · 3 fast: each step up is felt.
  static int _bandOf(double off) => off.abs() < stickDead ? 0 : (off.abs() < 45 ? 1 : (off.abs() < 90 ? 2 : 3));

  bool get _canStick => ((Host.I.config.value?['caps'] as List?) ?? const []).contains('pad-scroll');
  /// `same`: pushing up scrolls up — the page moves as a finger moving down would move it.
  double get _dirSign => '${_prefs['edge_dir'] ?? 'same'}' == 'reverse' ? -1 : 1;

  void _scrollClaim(_F f) {
    final e = _edge!;
    e.timer?.cancel();
    _mode = 'escroll';
    edgeGlow.value = null;
    final side = e.scroll!;
    final x = f.from.dx, gx = side == 'right' ? x - _ghostGap : x + _ghostGap;
    final stick = _canStick;
    final es = (id: e.id, ghost: _ghostBase + e.id, side: side, x: x, gx: gx, anchor: f.from.dy, size: f.size, stick: stick);
    _es = es;
    _edge = null;
    _notch = 0;
    _lastY = f.at.dy;
    _band = 0;
    scrollFelt.add('drop');
    if (stick) {
      // A scroll stick: held away from where it landed, it keeps scrolling, faster the further.
      Pad.I.send({'k': 'scroll', 'dx': 0.0, 'dy': 0.0, 'phase': 'began'});
      _lastTick = _now;
      _stick = Timer.periodic(const Duration(milliseconds: 16), (_) => _stickTick());
      _stickMove(f.at.dy);
      return;
    }
    final (w, h) = (f.size.width, f.size.height);
    // Both land where the finger did, then move with it: the computer reads two fingers scrolling.
    PadTouch.I.down(es.id, x, f.from.dy, w, h);
    PadTouch.I.down(es.ghost, gx, f.from.dy, w, h);
    scheduleMicrotask(() => _scrollMove(f.at.dy));
    edgeScroll.value = (side: side, anchor: f.at.dy, y: f.at.dy, speed: 0);
  }

  void _stickMove(double y) {
    final es = _es;
    if (es == null) return;
    _off = y - es.anchor;
    final band = _bandOf(_off);
    if (band > _band) scrollFelt.add(band == 1 ? 'scroll' : 'drop');
    _band = band;
    edgeScroll.value = (side: es.side, anchor: es.anchor, y: y, speed: (stickSpeed(_off).abs() / 2600).clamp(0, 1));
  }

  void _stickTick() {
    final now = _now;
    final dt = (now - _lastTick) / 1000;
    _lastTick = now;
    final v = -stickSpeed(_off) * _dirSign;
    if (v == 0) return;
    final dy = v * dt;
    Pad.I.send({'k': 'scroll', 'dx': 0.0, 'dy': dy, 'phase': 'changed'});
    // A notch per stretch scrolled — a wheel turning under the finger — never a buzz.
    _notch += dy.abs();
    if (_notch >= _stickNotchPx && now - _lastFelt >= _stickFeelMs) {
      _notch = 0;
      _lastFelt = now;
      scrollFelt.add('scroll');
    }
  }

  void _scrollMove(double y) {
    final es = _es;
    if (es == null) return;
    if (es.stick) return _stickMove(y);
    // Along the edge only: x stays where it landed, so a scroll never drifts sideways.
    PadTouch.I.move(es.id, es.x, y, es.size.width, es.size.height);
    PadTouch.I.move(es.ghost, es.gx, y, es.size.width, es.size.height);
    edgeScroll.value = (side: es.side, anchor: y, y: y, speed: 0);
    _notch += (y - _lastY).abs();
    _lastY = y;
    if (_notch >= _notchPx) {
      _notch = 0;
      scrollFelt.add('scroll');
    }
  }

  void _scrollEnd() {
    final es = _es;
    if (es == null) return;
    _es = null;
    if (es.stick) {
      _stick?.cancel();
      _stick = null;
      _off = 0;
      Pad.I.send({'k': 'scroll', 'dx': 0.0, 'dy': 0.0, 'phase': 'ended'});
      if (_band > 0) scrollFelt.add('scroll'); // let go: the stick springs home
    } else {
      PadTouch.I.up(es.id);
      PadTouch.I.up(es.ghost);
    }
    edgeScroll.value = null;
  }

  (double, int) get _dist => _distance['${_prefs['gesture_distance'] ?? 'medium'}'] ?? _distance['medium']!;

  Map<String, dynamic>? _action(String name) {
    final id = (_prefs['gestures'] as Map?)?[name];
    return id is String && id.isNotEmpty ? PadActions.I.byId(id) : null;
  }

  void _fire(Map<String, dynamic> action, Offset at) {
    fired.add((icon: '${action['icon'] ?? 'bolt'}', at: at));
    PadActions.I.run('${action['id']}');
  }

  // ── edge ──
  double _inward(_F f) => _edge!.side == 'edge-left' ? f.at.dx - f.from.dx : f.from.dx - f.at.dx;

  void _edgeRelease() {
    // Not an edge swipe: the held touches go to the pad as they happened.
    final e = _edge!;
    e.timer?.cancel();
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
        e.timer?.cancel();
        _mode = 'edge';
      }
      _fire(e.action!, f.at);
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
      final scroll = _scrollSide(p, s);
      if ((side != null && action != null) || scroll != null) {
        _mode = 'edge-hold';
        _edgeFired = false;
        // An edge swipe alone decides in 60 ms; with edge scrolling it waits for the finger to move.
        _edge = (
          side: action != null ? side! : 'none',
          id: id,
          held: [('down', p, s)],
          action: action,
          scroll: scroll,
          timer: scroll == null ? Timer(const Duration(milliseconds: _holdBackMs), _edgeDecide) : null,
        );
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
        final e = _edge!;
        if (id == e.id) {
          e.held.add(('move', p, s));
          if (e.scroll == null) {
            _edgeCheck(f);
          } else {
            final dx = (f.at.dx - f.from.dx).abs(), dy = (f.at.dy - f.from.dy).abs();
            if (dy >= _scrollLean && dy > dx * 1.2) {
              _scrollClaim(f);
            } else if (e.action != null && _inward(f) > 0 && _inward(f) >= dy) {
              _edgeCheck(f);
            } else if (f.moved >= _scrollLean) {
              _edgeRelease(); // the pointer, starting at the edge
            }
          }
        }
        return false;
      case 'escroll':
        if (_es != null && id == _es!.id) _scrollMove(p.dy);
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
    } else if (was == 'escroll') {
      if (_es != null && id == _es!.id) _scrollEnd();
      pass = false;
    } else if (was != null) {
      pass = false;
    } else {
      _twoCancel();
    }
    _fingers.remove(id);
    if (_fingers.isEmpty) {
      if (was == 'four' && _four != null) _fourEnd();
      _edge?.timer?.cancel();
      _edge = null;
      edgeGlow.value = null;
      _scrollEnd();
      _twoCancel();
      _mode = null;
    }
    return pass;
  }

  void reset() {
    _edge?.timer?.cancel();
    _scrollEnd();
    _twoCancel();
    _fingers.clear();
    _edge = null;
    _four = null;
    _mode = null;
    edgeGlow.value = null;
  }
}
