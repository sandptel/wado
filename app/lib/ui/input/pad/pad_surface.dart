// The touchpad's surface (port of pad_feel.js + the `.tparea` rules of pad.css): every finger goes
// to the computer as a real multi-touch touchpad; here each one lands as a ring that pops in with a
// soft halo, two are joined by a line with a midpoint, three or more show their count, and an
// accent glow follows their centre. A lift ripples the ring out.
import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter/scheduler.dart' show Ticker;

import '../../../wado/pad.dart';
import '../../../wado/pad_hints.dart';
import '../../../wado/settings.dart';
import '../../theme/look.dart';
import '../../theme/tokens.dart';
import '../../widgets/basics.dart';
import 'pad_actions_ui.dart';
import 'pad_coach.dart';

class _Finger {
  _Finger(this.at, this.born) : from = at;
  Offset at;
  /// Where this gesture started for it: reset whenever a finger comes or goes.
  Offset from;
  final int born;
}

class PadSurface extends StatefulWidget {
  const PadSurface({super.key, required this.marks, this.overlay});
  /// Draw the fingers (`marks` in the trackpad settings).
  final bool marks;
  /// The why-card or a note, over the surface.
  final Widget? overlay;
  @override
  State<PadSurface> createState() => _PadSurfaceState();
}

class _PadSurfaceState extends State<PadSurface> with SingleTickerProviderStateMixin {
  final _down = <int, _Finger>{};
  /// Lifted rings, rippling out: position and when.
  final _lifts = <(Offset, int)>[];
  final _sw = Stopwatch()..start();
  late final Ticker _tick = createTicker((_) {
    final now = _sw.elapsedMilliseconds;
    _lifts.removeWhere((l) => now - l.$2 > 140);
    // A lone finger keeps ticking until its hold ring has filled.
    final holding = _solo && _down.length == 1 && now - _since < holdMs + holdFillMs + 50;
    final busy = _lifts.isNotEmpty || holding || _down.values.any((f) => now - f.born < 220);
    if (!busy) _tick.stop();
    setState(() {});
  });
  bool _touched = false;
  int _since = 0;
  /// A two-finger set that read as pinch or scroll stays that, rather than flicker.
  String? _locked;
  /// One finger that came down alone (not the last of a set) may read as a hold.
  bool _solo = false;

  /// A finger came or went: the gesture starts over from where every finger now is.
  void _restart() {
    _since = _sw.elapsedMilliseconds;
    _locked = null;
    for (final f in _down.values) {
      f.from = f.at;
    }
  }

  PadHint? _hint() {
    var h = _down.isEmpty ? null : classify([for (final f in _down.values) (f.from, f.at)], _sw.elapsedMilliseconds - _since);
    if (_down.length == 2) {
      if (h != null && (h.kind == 'pinch' || h.kind == 'scroll')) _locked ??= h.kind;
      if (_locked != null && h?.kind != _locked) h = null;
    }
    if (h?.kind == 'hold' && !_solo) h = null;
    return h;
  }

  @override
  void dispose() {
    _tick.dispose();
    super.dispose();
  }

  void _wake() {
    if (!_tick.isActive) _tick.start();
  }

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return LayoutBuilder(builder: (context, box) {
      final w = box.maxWidth, h = box.maxHeight;
      final dpr = MediaQuery.devicePixelRatioOf(context);
      PadTouch.I.size((w * dpr).round(), (h * dpr).round());
      return Listener(
        behavior: HitTestBehavior.opaque,
        onPointerDown: (e) {
          PadTouch.I.down(e.pointer, e.localPosition.dx, e.localPosition.dy, w, h);
          _down[e.pointer] = _Finger(e.localPosition, _sw.elapsedMilliseconds);
          _restart();
          _solo = _down.length == 1;
          _touched = true;
          _wake();
          setState(() {});
        },
        onPointerMove: (e) {
          PadTouch.I.move(e.pointer, e.localPosition.dx, e.localPosition.dy, w, h);
          _down[e.pointer]?.at = e.localPosition;
          if (widget.marks) setState(() {});
        },
        onPointerUp: (e) => _lift(e.pointer),
        onPointerCancel: (e) => _lift(e.pointer),
        child: Container(
          clipBehavior: Clip.antiAlias,
          decoration: BoxDecoration(color: t.s1, borderRadius: BorderRadius.circular(t.r4), border: Border.all(color: t.line)),
          child: Stack(fit: StackFit.expand, children: [
            AnimatedOpacity(
              // Hidden under the coach, and once a finger has been down.
              opacity: _touched || !Settings.I.b('pad_coach_seen') ? 0 : 1,
              duration: t.dur,
              child: Center(
                child: Padding(
                  padding: const EdgeInsets.symmetric(horizontal: 24),
                  child: Text("Two fingers to scroll · your computer's gestures work here", textAlign: TextAlign.center, style: body(context, size: .8125, color: t.dim)),
                ),
              ),
            ),
            if (widget.marks)
              CustomPaint(painter: _Feel(t, {for (final e in _down.entries) e.key: e.value}, List.of(_lifts), _sw.elapsedMilliseconds, rem(context, .75), _hint())),
            const PadCoach(),
            const PadConfirm(),
            ?widget.overlay,
          ]),
        ),
      );
    });
  }

  void _lift(int id) {
    PadTouch.I.up(id);
    final f = _down.remove(id);
    if (f != null) _lifts.add((f.at, _sw.elapsedMilliseconds));
    _solo = false;
    _restart();
    _wake();
    setState(() {});
  }
}

class _Feel extends CustomPainter {
  _Feel(this.t, this.down, this.lifts, this.now, this.chipSize, this.hint);
  final Tokens t;
  final Map<int, _Finger> down;
  final List<(Offset, int)> lifts;
  final int now;
  final double chipSize;
  final PadHint? hint;

  @override
  void paint(Canvas canvas, Size size) {
    final a = t.accent;
    final pts = down.values.map((f) => f.at).toList();
    // The glow under the group's centre (`tp-ambient`).
    if (pts.isNotEmpty) {
      final c = pts.reduce((x, y) => x + y) / pts.length.toDouble();
      canvas.drawCircle(c, 180, Paint()..shader = RadialGradient(colors: [fade(a, 94), fade(a, 100)], stops: const [0, .7]).createShader(Rect.fromCircle(center: c, radius: 180)));
    }
    // Two fingers: the line between them, and its middle.
    if (pts.length == 2) {
      canvas.drawLine(pts[0], pts[1], Paint()
        ..color = fade(a, 55)
        ..strokeWidth = 2);
      canvas.drawCircle((pts[0] + pts[1]) / 2, 3, Paint()..color = fade(a, 40));
    }
    for (final f in down.values) {
      final age = (now - f.born).toDouble();
      // The halo blooms in over 220 ms; the ring pops from .6 over 160 ms.
      final hv = Tokens.spring.transform((age / 220).clamp(0, 1));
      canvas.drawCircle(f.at, 60 * (.6 + .4 * hv), Paint()
        ..shader = RadialGradient(colors: [fade(a, 85), fade(a, 100)], stops: const [0, .72]).createShader(Rect.fromCircle(center: f.at, radius: 60)));
      final rv = Tokens.popCurve.transform((age / 160).clamp(0, 1));
      _ring(canvas, f.at, .6 + .4 * rv, (age / 90).clamp(0, 1));
    }
    for (final (at, born) in lifts) {
      final v = ((now - born) / 140).clamp(0.0, 1.0);
      _ring(canvas, at, 1 + .5 * Curves.easeIn.transform(v), 1 - v);
    }
    if (hint != null) _hint(canvas, hint!);
    // Three or more: their count, at their centre.
    if (pts.length >= 3 && hint == null) {
      final c = pts.reduce((x, y) => x + y) / pts.length.toDouble();
      final tp = TextPainter(
        text: TextSpan(text: '${pts.length}', style: TextStyle(fontFamily: Tokens.mono, fontSize: chipSize, fontWeight: FontWeight.w700, color: t.onAccent)),
        textDirection: TextDirection.ltr,
      )..layout();
      final w = (tp.width + 10).clamp(22.0, 60.0);
      canvas.drawRRect(RRect.fromRectAndRadius(Rect.fromCenter(center: c, width: w, height: 22), const Radius.circular(11)), Paint()..color = fade(a, 20));
      tp.paint(canvas, c - Offset(tp.width / 2, tp.height / 2));
    }
  }

  /// `.tpg`: arrows leading past the fingers (scroll, swipe), or out and in along the pinch's line,
  /// or a held finger's ring filling; and a label, trailing behind or under them.
  void _hint(Canvas canvas, PadHint h) {
    final a = t.accent;
    final chev = Paint()
      ..style = PaintingStyle.stroke
      ..strokeWidth = 2.5
      ..strokeCap = StrokeCap.round
      ..strokeJoin = StrokeJoin.round
      ..color = a;
    // A chevron pointing along +x, centred at the origin (`M9 6l6 6-6 6`).
    final v = Path()
      ..moveTo(-3, -6)
      ..lineTo(3, 0)
      ..lineTo(-3, 6);
    void chevrons(Offset at, double angle, int count) {
      canvas.save();
      canvas.translate(at.dx, at.dy);
      canvas.rotate(angle);
      for (var i = 0; i < count; i++) {
        canvas.drawPath(v.shift(Offset(i * 10.0, 0)), chev..color = a.withValues(alpha: .45 + .55 * (i + 1) / count));
      }
      canvas.restore();
    }

    var off = const Offset(0, 64);
    switch (h.kind) {
      case 'scroll' || 'swipe':
        final ang = math.atan2(h.d.dy, h.d.dx), u = Offset(math.cos(ang), math.sin(ang));
        chevrons(h.at + u * (h.ahead + 30), ang, 3);
        off = Offset(-u.dx * (h.behind + 84), -u.dy * (h.behind + 50));
      case 'pinch':
        final u = Offset(math.cos(h.angle), math.sin(h.angle));
        chevrons(h.at - u * (h.half + 44), h.angle + (h.out ? math.pi : 0), 1);
        chevrons(h.at + u * (h.half + 44), h.angle + (h.out ? 0 : math.pi), 1);
        off = Offset(-u.dy * 60, u.dx * 60);
      case 'hold':
        final full = h.progress >= 1;
        canvas.drawCircle(h.at, 30, Paint()
          ..style = PaintingStyle.stroke
          ..strokeWidth = 2
          ..color = fade(a, 50));
        canvas.drawCircle(h.at, 30 * (.25 + .75 * h.progress), Paint()..color = full ? fade(a, 40) : fade(a, 75));
    }
    final label = h.label;
    if (label.isEmpty) return;
    final tp = TextPainter(
      text: TextSpan(text: label, style: TextStyle(fontSize: chipSize, fontWeight: FontWeight.w600, color: t.bright)),
      textDirection: TextDirection.ltr,
    )..layout();
    final r = Rect.fromCenter(center: h.at + off, width: tp.width + 20, height: tp.height + 10);
    canvas.drawRRect(RRect.fromRectAndRadius(r, const Radius.circular(999)), Paint()..color = Color.alphaBlend(fade(a, 82), t.s2));
    tp.paint(canvas, r.center - Offset(tp.width / 2, tp.height / 2));
  }

  /// `.tpfinger`: a 44 px ring, a solid core and a soft fill.
  void _ring(Canvas canvas, Offset at, double scale, double opacity) {
    if (opacity <= 0) return;
    final a = t.accent, r = 22 * scale;
    canvas.drawCircle(at, r, Paint()
      ..shader = RadialGradient(
        colors: [fade(a, 100 - 60 * opacity), fade(a, 100 - 22 * opacity), fade(a, 100 - 12 * opacity)],
        stops: const [.32, .41, 1],
      ).createShader(Rect.fromCircle(center: at, radius: r)));
    canvas.drawCircle(at, r, Paint()
      ..style = PaintingStyle.stroke
      ..strokeWidth = 2
      ..color = a.withValues(alpha: opacity));
  }

  @override
  bool shouldRepaint(_Feel o) => true;
}
