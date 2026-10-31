// The session at a glance (port of ui/cc/hero.rs): which computer, what shape, the verdict, and
// three live sparklines. The verdict is the conclusion and the sparklines are the evidence.
import 'package:flutter/material.dart';

import '../../../core/core_host.dart';
import '../../../wado/nav.dart';
import '../../../wado/relay_link.dart';
import '../../../wado/session.dart';
import '../../../wado/stats.dart';
import '../../motion/motion.dart';
import '../../theme/look.dart';
import '../../theme/tokens.dart';
import '../../widgets/basics.dart';

class CcHero extends StatelessWidget {
  const CcHero({super.key});
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final s = Session.I;
    return ListenableBuilder(
      listenable: Listenable.merge([s.stats, s.on, s.encoder, s.stage, RelayLink.I.phase, Stats.I.verdict]),
      builder: (context, _) {
        final on = s.on.value;
        final c = s.applied ?? Session.config();
        final st = s.stats.value;
        final sw = on && s.encoder.value.$1 == 'software';
        final h = Stats.I.verdict.value;
        final (verdict, tone) = !on
            ? ('not streaming', t.dim)
            : switch (h.state) { 'bad' => (h.side, t.bad), 'warn' => (h.side, t.base[0xA]), _ => ('healthy', t.good) };
        final need = h.needKbps;
        String n(double? v, [int d = 0]) => v == null || (on && v <= 0 && d == 0 && st == null) ? '—' : v.toStringAsFixed(d);
        final fpsMax = ((c['fps'] ?? 60) as num).toDouble();
        final stage = on ? s.stage.value.clamp(0, 4) : RelayLink.I.phase.value.clamp(0, 2);
        return Container(
          padding: const EdgeInsets.all(16),
          decoration: BoxDecoration(
            color: t.s1,
            borderRadius: BorderRadius.circular(t.r3),
            border: Border.all(color: t.line),
            gradient: LinearGradient(begin: Alignment.topLeft, end: Alignment.bottomRight, colors: [
              Color.alphaBlend(fade(t.accent, 80), t.s1),
              Color.alphaBlend(fade(t.base[0xE], 88), t.s1),
            ]),
          ),
          child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
            Row(children: [
              Container(
                width: 42,
                height: 42,
                decoration: BoxDecoration(color: t.accent, borderRadius: BorderRadius.circular(t.r2)),
                child: Center(child: WIcon('server', color: t.onAccent)),
              ),
              const SizedBox(width: 12),
              Expanded(
                child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                  Text(CoreHost.I.computer(Nav.I.selected)?.name ?? 'Computer', maxLines: 1, overflow: TextOverflow.ellipsis, style: display(context, rem(context, 1.1875), ls: -.19).copyWith(height: 1.1)),
                  Text('${c['width']}×${c['height']} · ${c['fps']} fps · ${c['scale']}×', style: mono(context, size: .75, color: t.dim)),
                ]),
              ),
              Container(
                padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 5),
                decoration: BoxDecoration(color: fade(tone, 84), borderRadius: BorderRadius.circular(999)),
                child: Row(mainAxisSize: MainAxisSize.min, children: [
                  Breathe(on: on, child: Container(width: 7, height: 7, decoration: BoxDecoration(shape: BoxShape.circle, color: tone))),
                  const SizedBox(width: 6),
                  Text(verdict, style: body(context, size: .75, w: FontWeight.w600, color: tone)),
                ]),
              ),
            ]),
            // Invariant #5: not switchable, not dismissable — as long as it is true, it is here.
            if (sw) ...[
              const SizedBox(height: 12),
              Container(
                padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 8),
                decoration: BoxDecoration(color: fade(t.warn, 84), borderRadius: BorderRadius.circular(t.r1)),
                child: Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
                  WIcon('alert', size: 16, color: t.warn),
                  const SizedBox(width: 8),
                  Expanded(child: Text('Software encoding — no working GPU encoder. Higher CPU use and latency.', style: body(context, size: .75, color: t.bright))),
                ]),
              ),
            ],
            if (on) ...[
              const SizedBox(height: 12),
              Row(children: [
                Expanded(child: _Meter(k: 'frames', v: n(st?.fps), unit: 'fps', data: s.spark.fps, hi: fpsMax, hue: t.base[0xB])),
                const SizedBox(width: 8),
                Expanded(child: _Meter(k: 'ping', v: n(st?.rtt), unit: 'ms', data: s.spark.ping, hi: 150, hue: t.base[0xC])),
                const SizedBox(width: 8),
                Expanded(
                  child: _Meter(
                    k: 'stream',
                    v: st == null ? '—' : (st.kbps / 1000).toStringAsFixed(1),
                    unit: need != null && need > 0 ? '/${(need / 1000).toStringAsFixed(1)} Mb' : 'Mb',
                    data: s.spark.kbps,
                    hi: (need ?? 8000) * 1.2,
                    hue: t.base[0xD],
                  ),
                ),
              ]),
              if (h.fix.isNotEmpty) ...[const SizedBox(height: 12), Text(h.fix, style: body(context, size: .75, color: t.text))],
            ],
            const SizedBox(height: 12),
            Row(children: [
              for (var i = 0; i < 4; i++) ...[
                if (i > 0) const SizedBox(width: 4),
                Expanded(
                  child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                    AnimatedContainer(duration: t.dur, height: 4, decoration: BoxDecoration(color: i < stage ? t.good : t.s3, borderRadius: BorderRadius.circular(2))),
                    const SizedBox(height: 8),
                    Text(['RELAY', 'DAEMON', 'SESSION', 'VIDEO'][i], style: body(context, size: .625, color: t.dim).copyWith(letterSpacing: .6)),
                  ]),
                ),
              ],
            ]),
          ]),
        );
      },
    );
  }
}

class _Meter extends StatelessWidget {
  const _Meter({required this.k, required this.v, required this.unit, required this.data, required this.hi, required this.hue});
  final String k, v, unit;
  final List<double> data;
  final double hi;
  final Color hue;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return Container(
      padding: const EdgeInsets.fromLTRB(10, 8, 10, 6),
      decoration: BoxDecoration(color: fade(t.s0, 35), borderRadius: BorderRadius.circular(t.r2)),
      child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
        Text(k.toUpperCase(), style: body(context, size: .625, color: t.dim).copyWith(letterSpacing: .8)),
        Text.rich(
          TextSpan(text: v, children: [TextSpan(text: ' $unit', style: body(context, size: .625, w: FontWeight.w500, color: t.dim))]),
          maxLines: 1,
          style: display(context, rem(context, 1.0625)).copyWith(height: 1.1, fontFeatures: const [FontFeature.tabularFigures()]),
        ),
        const SizedBox(height: 4),
        SizedBox(height: 22, width: double.infinity, child: CustomPaint(painter: _Spark(List.of(data), hi, hue))),
      ]),
    );
  }
}

/// A sparkline in its box between 0 and `hi`: the area, the line, the last point.
class _Spark extends CustomPainter {
  _Spark(this.d, this.hi, this.c);
  final List<double> d;
  final double hi;
  final Color c;
  @override
  void paint(Canvas canvas, Size size) {
    final pts = <Offset>[
      for (var i = 0; i < d.length; i++) Offset(d.length < 2 ? size.width : i / (d.length - 1) * size.width, size.height * (20 - (d[i] / (hi <= 0 ? 1 : hi)).clamp(0, 1) * 18) / 22),
    ];
    if (pts.length < 2) {
      canvas.drawCircle(Offset(size.width, size.height / 2), 2, Paint()..color = c);
      return;
    }
    final line = Path()..addPolygon(pts, false);
    final area = Path.from(line)
      ..lineTo(size.width, size.height)
      ..lineTo(0, size.height)
      ..close();
    canvas.drawPath(area, Paint()..color = fade(c, 82));
    canvas.drawPath(line, Paint()
      ..color = c
      ..style = PaintingStyle.stroke
      ..strokeWidth = 1.5);
    canvas.drawCircle(pts.last, 2, Paint()..color = c);
  }

  @override
  bool shouldRepaint(_Spark o) => true;
}
