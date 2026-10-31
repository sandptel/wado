// What rides on the picture when Diagnostics asks for it (port of ui/stage.rs, ui/health.rs and
// js/overlay.js): the status pill with the encode pipeline's badge and the numbers, the health
// verdict, and the touch marks. Debug views — every one gated by its readout switch — except the
// pipeline badge, which is not a debug view. None of them ever takes a tap.
import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/scheduler.dart' show Ticker;

import '../../../wado/health.dart';
import '../../../wado/session.dart';
import '../../../wado/settings.dart';
import '../../../wado/stats.dart';
import '../../pages/diagnostics.dart' show readout;
import '../../theme/look.dart';
import '../../theme/tokens.dart';
import '../../widgets/basics.dart' show rem;

/// Label and tone for the active encode pipeline: the badge is how you notice you asked for
/// hardware and did not get it.
(String, int)? _badge(String p) => switch (p) {
  'vaapi-dmabuf' => ('⚡ zero-copy', 0xB),
  'vaapi-cpu' => ('HW · cpu-copy', 0xA),
  'x264-cpu' => ('SW · x264', 8),
  _ => null,
};

String _rate(double k) => k >= 1000 ? '${(k / 1000).toStringAsFixed(1)} Mbps' : '${k.round()} kbps';

/// Two rates compared, with one unit between them where both share it.
String _pair(double got, double need) {
  if (got >= 1000 && need >= 1000) return '${(got / 1000).toStringAsFixed(1)} / ${(need / 1000).toStringAsFixed(1)} Mbps';
  if (got < 1000 && need < 1000) return '${got.round()} / ${need.round()} kbps';
  return '${_rate(got)} / ${_rate(need)}';
}

/// Over the picture's own rect: health top-left, the pill top-right.
class Readouts extends StatelessWidget {
  const Readouts({super.key});
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final s = Session.I;
    return IgnorePointer(
      child: ListenableBuilder(
        listenable: Listenable.merge([Settings.I, Stats.I.sample, Stats.I.verdict, s.on, s.encoder, s.stagebar]),
        builder: (context, _) {
          if (!s.on.value) return const SizedBox.shrink();
          final narrow = MediaQuery.sizeOf(context).width < 520;
          final x = Stats.I.sample.value;
          final fps = readout('fps'), ping = readout('ping'), bar = readout('statusbar');
          final nums = [
            if (fps) x?.fps == null ? '— fps' : '${x!.fps!.round()} fps',
            if (ping) x?.ping == null ? '— ms' : '${x!.ping!.round()} ms',
            if (ping && x?.jbuf != null) '${x!.jbuf!.round()} ms buf',
          ].join(' · ');
          final badge = _badge(s.encoder.value.$2);
          final pill = bar || fps || ping;
          final mono = TextStyle(fontFamily: Tokens.mono, fontSize: rem(context, .6875), height: 1.5, fontFeatures: const [FontFeature.tabularFigures()]);
          final pillW = pill
              ? Container(
                  padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 4),
                  decoration: BoxDecoration(color: const Color(0x99000000), borderRadius: BorderRadius.circular(999)),
                  child: Row(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      if (bar && !narrow) ...[
                        Flexible(
                          child: Text(
                            s.stagebar.value,
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: TextStyle(fontSize: rem(context, .6875), color: t.base[4]),
                          ),
                        ),
                        const SizedBox(width: 8),
                      ],
                      if (badge != null)
                        Container(
                          margin: const EdgeInsets.only(right: 8),
                          padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 1),
                          decoration: BoxDecoration(
                            borderRadius: BorderRadius.circular(3),
                            border: Border.all(color: t.base[badge.$2]),
                          ),
                          child: Text(badge.$1, style: mono.copyWith(color: t.base[badge.$2])),
                        ),
                      if (nums.isNotEmpty)
                        Flexible(
                          child: Text(
                            nums,
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: mono.copyWith(color: t.base[0xB]),
                          ),
                        ),
                    ],
                  ),
                )
              : null;
          // Health in the top-left corner, the pill in the top-right — one under the other, so a
          // narrow picture never stacks them on top of each other.
          return Padding(
            padding: const EdgeInsets.all(8),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                if (readout('health'))
                  Align(
                    alignment: Alignment.topLeft,
                    child: _Health(v: Stats.I.verdict.value, narrow: narrow),
                  ),
                if (readout('health') && pillW != null) const SizedBox(height: 6),
                if (pillW != null) Align(alignment: Alignment.topRight, child: pillW),
              ],
            ),
          );
        },
      ),
    );
  }
}

class _Health extends StatelessWidget {
  const _Health({required this.v, required this.narrow});
  final Verdict v;
  final bool narrow;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final c = switch (v.state) {
      'bad' => t.base[8],
      'warn' => t.base[0xA],
      _ => t.base[0xB],
    };
    final ok = v.state == 'ok';
    final bw = v.gotKbps == null ? '' : (v.needKbps != null && v.needKbps! > 0 ? _pair(v.gotKbps!, v.needKbps!) : _rate(v.gotKbps!));
    final size = rem(context, narrow ? .6875 : .75);
    return Container(
      padding: EdgeInsets.fromLTRB(narrow ? 6 : 8, narrow ? 3 : 4, narrow ? 8 : 10, narrow ? 3 : 4),
      decoration: BoxDecoration(
        color: ok ? fade(t.base[0], 18) : Color.alphaBlend(fade(c, v.state == 'bad' ? 82 : 86), t.base[0]),
        borderRadius: BorderRadius.circular(t.r1),
        border: Border.all(color: ok ? Color.lerp(t.base[2], t.base[0xB], .4)! : c),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          _Dot(color: c, pulse: v.state == 'bad', size: narrow ? 7 : 8),
          SizedBox(width: narrow ? 6 : 8),
          Flexible(
            child: Text(
              ok ? 'healthy' : v.side,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: TextStyle(fontSize: size, fontWeight: FontWeight.w600, color: ok ? t.base[5] : c),
            ),
          ),
          if (!narrow && v.detail.isNotEmpty) ...[
            const SizedBox(width: 8),
            Flexible(
              child: Text(
                v.detail,
                maxLines: 2,
                style: TextStyle(fontSize: size, color: t.base[6]),
              ),
            ),
          ],
          if (v.fix.isNotEmpty) ...[
            SizedBox(width: narrow ? 6 : 8),
            Flexible(
              child: Container(
                padding: const EdgeInsets.symmetric(horizontal: 6),
                decoration: BoxDecoration(
                  borderRadius: BorderRadius.circular(999),
                  border: Border.all(color: c),
                ),
                child: Text(
                  v.fix,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(fontSize: rem(context, .6875), color: c),
                ),
              ),
            ),
          ],
          if (bw.isNotEmpty && !narrow) ...[
            const SizedBox(width: 8),
            Text(
              bw,
              style: TextStyle(fontSize: size, color: t.base[4], fontFeatures: const [FontFeature.tabularFigures()]),
            ),
          ],
        ],
      ),
    );
  }
}

class _Dot extends StatefulWidget {
  const _Dot({required this.color, required this.pulse, required this.size});
  final Color color;
  final bool pulse;
  final double size;
  @override
  State<_Dot> createState() => _DotState();
}

class _DotState extends State<_Dot> with SingleTickerProviderStateMixin {
  late final _c = AnimationController(vsync: this, duration: const Duration(milliseconds: 1400));
  @override
  void dispose() {
    _c.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    widget.pulse ? (_c.isAnimating ? null : _c.repeat()) : _c.stop();
    return RepaintBoundary(child: _pulse());
  }

  Widget _pulse() {
    return AnimatedBuilder(
      animation: _c,
      builder: (context, _) => Opacity(
        opacity: widget.pulse ? 1 - .65 * (1 - (2 * _c.value - 1).abs()) : 1,
        child: Container(
          width: widget.size,
          height: widget.size,
          decoration: BoxDecoration(shape: BoxShape.circle, color: widget.color),
        ),
      ),
    );
  }
}

/// The touch marks (overlay.js): a green dot where a finger lands, a blue trail where it moves,
/// each fading in a quarter second. Fed by the picture's input with global positions.
class TouchMarks extends StatefulWidget {
  const TouchMarks({super.key});
  static final _feed = StreamController<(int, Offset, String)>.broadcast();

  /// `phase`: down · move · up.
  static void touch(int id, Offset at, String phase) {
    if (readout('touches')) _feed.add((id, at, phase));
  }

  @override
  State<TouchMarks> createState() => _TouchMarksState();
}

class _Mark {
  _Mark(this.a, this.b, this.born);
  final Offset a;
  final Offset? b;
  final int born;
}

class _TouchMarksState extends State<TouchMarks> with SingleTickerProviderStateMixin {
  final _marks = <_Mark>[];
  final _last = <int, Offset>{};
  final _sw = Stopwatch()..start();
  late final StreamSubscription _sub;
  late final Ticker _tick = createTicker((_) {
    _marks.removeWhere((m) => _sw.elapsedMilliseconds - m.born > 280);
    if (_marks.isEmpty) _tick.stop();
    setState(() {});
  });

  @override
  void initState() {
    super.initState();
    _sub = TouchMarks._feed.stream.listen((e) {
      final (id, at, phase) = e;
      final now = _sw.elapsedMilliseconds;
      if (phase == 'down') _marks.add(_Mark(at, null, now));
      if (phase == 'move' && _last[id] != null) _marks.add(_Mark(_last[id]!, at, now));
      phase == 'up' ? _last.remove(id) : _last[id] = at;
      if (!_tick.isActive) _tick.start();
    });
  }

  @override
  void dispose() {
    _sub.cancel();
    _tick.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final box = context.findRenderObject() as RenderBox?;
    final o = box?.hasSize == true ? box!.localToGlobal(Offset.zero) : Offset.zero;
    return IgnorePointer(
      child: CustomPaint(size: Size.infinite, painter: _MarksPainter(List.of(_marks), o)),
    );
  }
}

class _MarksPainter extends CustomPainter {
  _MarksPainter(this.marks, this.origin);
  final List<_Mark> marks;
  final Offset origin;
  @override
  void paint(Canvas canvas, Size size) {
    for (final m in marks) {
      if (m.b == null) {
        canvas.drawCircle(m.a - origin, 18, Paint()..color = const Color(0x732EC878));
      } else {
        canvas.drawLine(
          m.a - origin,
          m.b! - origin,
          Paint()
            ..color = const Color(0x8C40AAFF)
            ..strokeWidth = 6
            ..strokeCap = StrokeCap.round,
        );
      }
    }
  }

  @override
  bool shouldRepaint(_MarksPainter o) => true;
}
