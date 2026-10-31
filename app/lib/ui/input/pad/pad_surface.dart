// The touchpad's surface (port of pad_feel.js + the `.tparea` rules of pad.css): every finger goes
// to the computer as a real multi-touch touchpad; here each one lands as a ring that pops in with a
// soft halo, two are joined by a line with a midpoint, three or more show their count, and an
// accent glow follows their centre. A lift ripples the ring out.
import 'package:flutter/material.dart';
import 'package:flutter/scheduler.dart' show Ticker;

import '../../../wado/pad.dart';
import '../../../wado/settings.dart';
import '../../theme/look.dart';
import '../../theme/tokens.dart';
import '../../widgets/basics.dart';
import 'pad_actions_ui.dart';
import 'pad_coach.dart';

class _Finger {
  _Finger(this.at, this.born);
  Offset at;
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
    final busy = _lifts.isNotEmpty || _down.values.any((f) => now - f.born < 220);
    if (!busy) _tick.stop();
    setState(() {});
  });
  bool _touched = false;

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
              CustomPaint(painter: _Feel(t, {for (final e in _down.entries) e.key: e.value}, List.of(_lifts), _sw.elapsedMilliseconds, rem(context, .75))),
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
    _wake();
    setState(() {});
  }
}

class _Feel extends CustomPainter {
  _Feel(this.t, this.down, this.lifts, this.now, this.chipSize);
  final Tokens t;
  final Map<int, _Finger> down;
  final List<(Offset, int)> lifts;
  final int now;
  final double chipSize;

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
    // Three or more: their count, at their centre.
    if (pts.length >= 3) {
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
