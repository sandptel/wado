// Input to the session (port of js/input_core.js + input_coalesce.js + input_units.js +
// input_accel.js + targets.js + input_keyboard.js's map). Coordinates are normalised 0..1 against
// the picture's displayed content rect — wherever the console or the cockpit put it — so the
// server scales 1:1 to the output. wado renders no cursor.
//
// Invariant 1: input never rides behind video, nor behind anything that can head-of-line block
// it. Absolute, latest-wins moves go on the zero-retransmit motion channel; everything else
// reliable and ordered — and with "Redundant input", discrete events also as one numbered,
// unordered copy on the fast channel (the daemon takes whichever arrives first).
import 'dart:async';
import 'dart:convert';
import 'dart:math' as math;

import 'package:flutter/scheduler.dart';
import 'package:flutter/services.dart';
import 'package:flutter_webrtc/flutter_webrtc.dart';

import 'relay_link.dart';
import 'session.dart';
import 'settings.dart';

class Input {
  Input._() {
    RelayLink.I.on('targets', (m) => _answer((m['seq'] ?? 0) as int, m['targets'] as List?));
  }
  static final I = Input._();

  /// The picture's displayed content, in global logical px (set by the session view each layout,
  /// including mid-morph), and the video's own size in video px.
  Rect content = Rect.zero;
  Size video = Size.zero;
  int _seq = 0;

  static bool _isMotion(Map<String, dynamic> o) => o['t'] == 'pointer_motion' || (o['t'] == 'window_drag' && o['phase'] == 'motion');
  static bool _isDiscrete(Map<String, dynamic> o) {
    final t = o['t'];
    return t == 'key' || t == 'button' || t == 'cancel_touch' || (t == 'touch' && o['phase'] != 'motion') || (t == 'scroll' && o['stop'] == true) || (t == 'window_drag' && o['phase'] != 'motion');
  }

  static bool _open(RTCDataChannel? c) => c != null && c.state == RTCDataChannelState.RTCDataChannelOpen;

  /// Send one input event on whichever channel suits it.
  void send(Map<String, dynamic> o) {
    final s = Session.I;
    final motion = _isMotion(o);
    if (!motion && Settings.I.b('redundant_input') && _isDiscrete(o) && _open(s.fastDC) && _open(s.inputDC)) {
      final msg = RTCDataChannelMessage(jsonEncode({...o, 'q': ++_seq}));
      s.fastDC!.send(msg);
      s.inputDC!.send(msg);
      return;
    }
    var dc = motion ? s.motionDC : s.inputDC;
    if (motion && !_open(dc)) dc = s.inputDC;
    if (_open(dc)) {
      dc!.send(RTCDataChannelMessage(jsonEncode(o)));
    } else if (!motion) {
      // No picture path yet: the relay carries it (late beats lost for a press).
      RelayLink.I.send({'type': 'input', 'event': o});
    }
  }

  // ── coalescing: at most one positional send per frame per kind, newest wins ──
  final _pending = <String, Map<String, dynamic>>{};
  bool _scheduled = false;

  void _flush() {
    _scheduled = false;
    if (_pending.isEmpty) return;
    final batch = [..._pending.values];
    _pending.clear();
    for (final p in batch) {
      send(p);
    }
  }

  void _arm() {
    if (_scheduled) return;
    _scheduled = true;
    SchedulerBinding.instance.scheduleFrameCallback((_) => _flush());
    SchedulerBinding.instance.scheduleFrame();
  }

  /// A positional update: only the newest per kind survives the frame.
  void queue(String kind, Map<String, dynamic> p) {
    _pending[kind] = p;
    _arm();
  }

  /// An additive update: dx/dy sum into what is pending (relative motion is not newest-wins).
  void add(String kind, Map<String, dynamic> p) {
    final prev = _pending[kind];
    if (prev != null) {
      p['dx'] = (p['dx'] as num) + (prev['dx'] as num);
      p['dy'] = (p['dy'] as num) + (prev['dy'] as num);
    }
    _pending[kind] = p;
    _arm();
  }

  /// Send at once, flushing what is queued so ordering holds (presses, lifts, keys).
  void now(Map<String, dynamic> p) {
    _flush();
    send(p);
  }

  /// Forget queued updates without sending (teardown: a stale position must not land later).
  void clear() => _pending.clear();

  // ── units ──

  /// A global point as 0..1 within the picture's content; null before there is a picture.
  Offset? norm(Offset g) {
    final c = content;
    if (c.isEmpty) return null;
    return Offset(((g.dx - c.left) / c.width).clamp(0.0, 1.0), ((g.dy - c.top) / c.height).clamp(0.0, 1.0));
  }

  /// Logical px → compositor logical px: a finger moving N px across the glass scrolls the
  /// content N px across the glass (undo the picture's fit, then the session's own scale).
  double get cssToLogical {
    if (content.isEmpty || video.isEmpty) return 1;
    final fit = content.width / video.width;
    if (!(fit > 0)) return 1;
    final s = Session.I.outputScale;
    return 1 / (fit * (s > 0 ? s : 1));
  }

  // ── scroll acceleration (input_accel.js) ──
  DateTime _lastScroll = DateTime(2000);
  double gain(double magnitude) {
    final now = DateTime.now();
    final gap = now.difference(_lastScroll).inMicroseconds / 1000;
    _lastScroll = now;
    if (!(gap > 0) || gap > 200) return 1;
    final v = magnitude.abs() / math.max(gap, 4);
    return 1 + (2.0 - 1) * math.min(v / 1.2, 1);
  }

  /// A finger-source scroll of (dx, dy) logical px at global point `at`; the point, for `stop`.
  Offset? scrollBy(double dx, double dy, Offset at) {
    final n = norm(at);
    if (n == null) return null;
    final s = Settings.I;
    final sign = s.b('natural_scroll') ? -1 : 1;
    final speed = s.d('scroll_speed') * gain(math.sqrt(dx * dx + dy * dy)) * cssToLogical;
    send({'t': 'scroll', 'x': n.dx, 'y': n.dy, 'dx': -dx * speed * sign, 'dy': -dy * speed * sign, 'source': 'finger'});
    return n;
  }

  void scrollStop(Offset? n) {
    if (n == null) return;
    send({'t': 'scroll', 'x': n.dx, 'y': n.dy, 'dx': 0, 'dy': 0, 'source': 'finger', 'stop': true});
  }

  void click(Offset n, String button) {
    now({'t': 'button', 'x': n.dx, 'y': n.dy, 'button': button, 'pressed': true});
    send({'t': 'button', 'x': n.dx, 'y': n.dy, 'button': button, 'pressed': false});
  }

  // ── tap targets (targets.js): ask on touch-down, the answer waits for the lift ──
  final _tickets = <int, Completer<List?>>{};
  int _tseq = 0;

  int ask(Offset g) {
    final n = norm(g);
    final seq = ++_tseq & 0xffffffff;
    final c = Completer<List?>();
    _tickets[seq] = c;
    // Fingertip radius: 22 logical px (~7 mm). Nothing to ask with: "no tree".
    if (n == null || !RelayLink.I.send({'type': 'targets_request', 'seq': seq, 'x': n.dx, 'y': n.dy, 'r': 22 / content.width})) c.complete(null);
    if (_tickets.length > 16) _tickets.remove(_tickets.keys.first);
    return seq;
  }

  /// The answer, or null when there is none within 70 ms of the lift (a plain tap).
  Future<List?> wait(int seq) => _tickets[seq]?.future.timeout(const Duration(milliseconds: 70), onTimeout: () => null) ?? Future.value(null);

  void _answer(int seq, List? targets) {
    final c = _tickets[seq];
    if (c != null && !c.isCompleted) c.complete(targets);
  }

  /// One target near the finger: click its centre (the magnetic tap); else where the finger was.
  static Offset decide(Offset n, List? targets) {
    if (targets == null || targets.length != 1) return n;
    final t = targets.first as Map;
    return Offset(((t['x'] as num) + (t['w'] as num) / 2).toDouble(), ((t['y'] as num) + (t['h'] as num) / 2).toDouble());
  }

  // ── the keyboard (input_keyboard.js `CODE_TO_EVDEV`) ──
  final _down = <int>{};

  bool key(KeyEvent e) {
    final code = evdev[e.physicalKey];
    if (code == null) return false;
    if (e is KeyRepeatEvent) return true;
    final pressed = e is KeyDownEvent;
    pressed ? _down.add(code) : _down.remove(code);
    now({'t': 'key', 'code': code, 'pressed': pressed});
    return true;
  }

  void releaseKeys() {
    for (final c in _down) {
      send({'t': 'key', 'code': c, 'pressed': false});
    }
    _down.clear();
  }
}

/// Physical key → Linux evdev code.
final evdev = <PhysicalKeyboardKey, int>{
  PhysicalKeyboardKey.keyA: 30, PhysicalKeyboardKey.keyB: 48, PhysicalKeyboardKey.keyC: 46, PhysicalKeyboardKey.keyD: 32, PhysicalKeyboardKey.keyE: 18,
  PhysicalKeyboardKey.keyF: 33, PhysicalKeyboardKey.keyG: 34, PhysicalKeyboardKey.keyH: 35, PhysicalKeyboardKey.keyI: 23, PhysicalKeyboardKey.keyJ: 36,
  PhysicalKeyboardKey.keyK: 37, PhysicalKeyboardKey.keyL: 38, PhysicalKeyboardKey.keyM: 50, PhysicalKeyboardKey.keyN: 49, PhysicalKeyboardKey.keyO: 24,
  PhysicalKeyboardKey.keyP: 25, PhysicalKeyboardKey.keyQ: 16, PhysicalKeyboardKey.keyR: 19, PhysicalKeyboardKey.keyS: 31, PhysicalKeyboardKey.keyT: 20,
  PhysicalKeyboardKey.keyU: 22, PhysicalKeyboardKey.keyV: 47, PhysicalKeyboardKey.keyW: 17, PhysicalKeyboardKey.keyX: 45, PhysicalKeyboardKey.keyY: 21,
  PhysicalKeyboardKey.keyZ: 44,
  PhysicalKeyboardKey.digit1: 2, PhysicalKeyboardKey.digit2: 3, PhysicalKeyboardKey.digit3: 4, PhysicalKeyboardKey.digit4: 5, PhysicalKeyboardKey.digit5: 6,
  PhysicalKeyboardKey.digit6: 7, PhysicalKeyboardKey.digit7: 8, PhysicalKeyboardKey.digit8: 9, PhysicalKeyboardKey.digit9: 10, PhysicalKeyboardKey.digit0: 11,
  PhysicalKeyboardKey.enter: 28, PhysicalKeyboardKey.escape: 1, PhysicalKeyboardKey.backspace: 14, PhysicalKeyboardKey.tab: 15, PhysicalKeyboardKey.space: 57,
  PhysicalKeyboardKey.minus: 12, PhysicalKeyboardKey.equal: 13, PhysicalKeyboardKey.bracketLeft: 26, PhysicalKeyboardKey.bracketRight: 27,
  PhysicalKeyboardKey.backslash: 43, PhysicalKeyboardKey.semicolon: 39, PhysicalKeyboardKey.quote: 40, PhysicalKeyboardKey.backquote: 41,
  PhysicalKeyboardKey.comma: 51, PhysicalKeyboardKey.period: 52, PhysicalKeyboardKey.slash: 53,
  PhysicalKeyboardKey.shiftLeft: 42, PhysicalKeyboardKey.shiftRight: 54, PhysicalKeyboardKey.controlLeft: 29, PhysicalKeyboardKey.controlRight: 97,
  PhysicalKeyboardKey.altLeft: 56, PhysicalKeyboardKey.altRight: 100, PhysicalKeyboardKey.metaLeft: 125, PhysicalKeyboardKey.metaRight: 126,
  PhysicalKeyboardKey.capsLock: 58,
  PhysicalKeyboardKey.arrowUp: 103, PhysicalKeyboardKey.arrowDown: 108, PhysicalKeyboardKey.arrowLeft: 105, PhysicalKeyboardKey.arrowRight: 106,
  PhysicalKeyboardKey.home: 102, PhysicalKeyboardKey.end: 107, PhysicalKeyboardKey.pageUp: 104, PhysicalKeyboardKey.pageDown: 109,
  PhysicalKeyboardKey.insert: 110, PhysicalKeyboardKey.delete: 111,
  PhysicalKeyboardKey.f1: 59, PhysicalKeyboardKey.f2: 60, PhysicalKeyboardKey.f3: 61, PhysicalKeyboardKey.f4: 62, PhysicalKeyboardKey.f5: 63, PhysicalKeyboardKey.f6: 64,
  PhysicalKeyboardKey.f7: 65, PhysicalKeyboardKey.f8: 66, PhysicalKeyboardKey.f9: 67, PhysicalKeyboardKey.f10: 68, PhysicalKeyboardKey.f11: 87, PhysicalKeyboardKey.f12: 88,
};
