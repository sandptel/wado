// The touchpad's quick actions on screen (port of pad_rail.js `actionButton` + pad_confirm.js):
// a tile per chosen action — its glyph, a spinner while it runs, a drawn tick when it worked, a
// shake when it did not — and the confirm disc that rises over the surface with its name.
import 'dart:async';

import 'package:flutter/material.dart';

import '../../../wado/pad_actions.dart';
import '../../theme/look.dart';
import '../../theme/tokens.dart';
import '../../widgets/basics.dart';
import '../../widgets/icon_paths.dart';
import '../../pages/input_pages.dart' show TouchpadSettings;

/// What the confirm disc shows: the action and whether it worked.
final padConfirm = ValueNotifier<({Map<String, dynamic> a, bool ok, int n})?>(null);
var _n = 0;

String _icon(Map<String, dynamic> a) => iconPaths.containsKey(a['icon']) ? '${a['icon']}' : 'dots';

/// The chosen actions, in the chosen order, that the computer offers.
List<Map<String, dynamic>> padQuick() {
  final quick = (TouchpadSettings.prefs()['quick'] as List?)?.cast<String>() ?? const ['workspace-prev', 'workspace-next', 'fullscreen', 'play-pause'];
  final seen = <String>{};
  return [for (final id in quick) if (seen.add(id) && PadActions.I.byId(id) != null) PadActions.I.byId(id)!];
}

/// The rail's (column) or the bar's (row of pills) actions, or the reason there are none.
class PadActionList extends StatelessWidget {
  const PadActionList({super.key, required this.bar});
  final bool bar;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return ListenableBuilder(
      listenable: Listenable.merge([PadActions.I.list, PadActions.I.denied]),
      builder: (context, _) {
        final denied = PadActions.I.denied.value;
        final items = padQuick();
        Widget note(String s) => Padding(padding: const EdgeInsets.all(4), child: Text(s, style: body(context, size: .75, color: t.dim).copyWith(height: 1.45)));
        if (denied.isNotEmpty) return note(denied);
        if (items.isEmpty) return note(PadActions.I.list.value.isNotEmpty ? 'No quick actions yet — pick some in settings.' : 'Asking the computer what it can do…');
        final tiles = [for (final a in items) _Act(a: a, bar: bar)];
        return bar
            ? SingleChildScrollView(scrollDirection: Axis.horizontal, child: Row(children: [for (final w in tiles) Padding(padding: const EdgeInsets.only(right: 8), child: w)]))
            : Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [for (final w in tiles) Padding(padding: const EdgeInsets.only(bottom: 8), child: w)]);
      },
    );
  }
}

class _Act extends StatefulWidget {
  const _Act({required this.a, required this.bar});
  final Map<String, dynamic> a;
  final bool bar;
  @override
  State<_Act> createState() => _ActState();
}

class _ActState extends State<_Act> with TickerProviderStateMixin {
  String _st = '';
  late final _spin = AnimationController(vsync: this, duration: const Duration(milliseconds: 700));
  late final _mark = AnimationController(vsync: this, duration: const Duration(milliseconds: 280));

  @override
  void dispose() {
    _spin.dispose();
    _mark.dispose();
    super.dispose();
  }

  Future<void> _run() async {
    if (_st == 'run') return;
    final t0 = DateTime.now();
    setState(() => _st = 'run');
    _spin.repeat();
    final res = await PadActions.I.run('${widget.a['id']}');
    // Long enough to read as "it ran" (SPIN_MIN_MS).
    final wait = 110 - DateTime.now().difference(t0).inMilliseconds;
    if (wait > 0) await Future.delayed(Duration(milliseconds: wait));
    if (!mounted) return;
    _spin.stop();
    setState(() => _st = res.ok ? 'ok' : 'bad');
    _mark.forward(from: 0);
    padConfirm.value = (a: widget.a, ok: res.ok, n: ++_n);
    Timer(const Duration(milliseconds: 280), () {
      if (mounted && _st != 'run') setState(() => _st = '');
    });
  }

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final ok = _st == 'ok', bad = _st == 'bad', run = _st == 'run';
    final bg = ok ? Color.alphaBlend(fade(t.good, 80), t.s2) : (bad ? Color.alphaBlend(fade(t.bad, 75), t.s2) : t.s2);
    final glyph = SizedBox(
      width: 24,
      height: 24,
      child: Stack(alignment: Alignment.center, children: [
        AnimatedOpacity(opacity: run || ok ? 0 : 1, duration: const Duration(milliseconds: 120), child: WIcon(_icon(widget.a), size: 22, color: bad ? t.bad : t.bright)),
        if (run) RotationTransition(turns: _spin, child: SizedBox(width: 22, height: 22, child: CircularProgressIndicator(strokeWidth: 2.5, value: .3, color: t.accent, backgroundColor: fade(t.accent, 75)))),
        if (ok) AnimatedBuilder(animation: _mark, builder: (_, _) => CustomPaint(size: const Size(22, 22), painter: _Tick(t.good, Tokens.spring.transform((_mark.value * 280 / 220).clamp(0, 1))))),
      ]),
    );
    final label = Text('${widget.a['name'] ?? ''}', maxLines: widget.bar ? 1 : 2, overflow: TextOverflow.ellipsis, textAlign: TextAlign.center, style: body(context, size: .75, w: FontWeight.w600, color: bad ? t.bad : t.bright).copyWith(height: 1.25));
    return Press(
      onTap: _run,
      label: '${widget.a['name'] ?? ''}',
      child: AnimatedBuilder(
        animation: _mark,
        builder: (context, child) {
          final v = _mark.value;
          // ok: a pop to 1.06 at 45%; bad: a shake.
          final scale = ok ? 1 + .06 * (v < .45 ? v / .45 : (1 - v) / .55) : 1.0;
          final dx = bad ? (v < .2 ? -5 * v / .2 : v < .45 ? -5 + 10 * (v - .2) / .25 : v < .7 ? 5 - 8 * (v - .45) / .25 : v < .9 ? -3 + 5 * (v - .7) / .2 : 2 * (1 - v) / .1) : 0.0;
          return Transform.translate(offset: Offset(dx, 0), child: Transform.scale(scale: scale, child: child));
        },
        child: AnimatedContainer(
          duration: t.dur,
          constraints: BoxConstraints(minHeight: widget.bar ? 48 : 64),
          padding: widget.bar ? const EdgeInsets.fromLTRB(10, 0, 14, 0) : const EdgeInsets.symmetric(horizontal: 6, vertical: 8),
          decoration: BoxDecoration(color: bg, borderRadius: BorderRadius.circular(widget.bar ? 999 : t.r2)),
          child: widget.bar
              ? Row(mainAxisSize: MainAxisSize.min, children: [glyph, const SizedBox(width: 8), label])
              : Column(mainAxisAlignment: MainAxisAlignment.center, children: [glyph, const SizedBox(height: 4), label]),
        ),
      ),
    );
  }
}

class _Tick extends CustomPainter {
  _Tick(this.c, this.v);
  final Color c;
  final double v;
  @override
  void paint(Canvas canvas, Size s) {
    final k = s.width / 24;
    final path = Path()
      ..moveTo(20 * k, 6 * k)
      ..lineTo(9 * k, 17 * k)
      ..lineTo(4 * k, 12 * k);
    final m = path.computeMetrics().toList();
    final total = m.fold<double>(0, (a, x) => a + x.length);
    var left = total * v;
    final p = Paint()
      ..color = c
      ..style = PaintingStyle.stroke
      ..strokeWidth = 2.6
      ..strokeCap = StrokeCap.round
      ..strokeJoin = StrokeJoin.round;
    for (final x in m) {
      if (left <= 0) break;
      canvas.drawPath(x.extractPath(0, left.clamp(0, x.length)), p);
      left -= x.length;
    }
  }

  @override
  bool shouldRepaint(_Tick o) => o.v != v || o.c != c;
}

/// `.tpconfirm`: the action's disc with a tick or a cross badge, and its name, in the middle of the
/// surface for 620 ms.
class PadConfirm extends StatelessWidget {
  const PadConfirm({super.key});
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return IgnorePointer(
      child: ValueListenableBuilder(
        valueListenable: padConfirm,
        builder: (context, c, _) {
          if (c == null) return const SizedBox.shrink();
          return Center(
            child: TweenAnimationBuilder<double>(
              key: ValueKey(c.n),
              tween: Tween(begin: 0, end: 620),
              duration: const Duration(milliseconds: 620),
              onEnd: () {
                if (padConfirm.value?.n == c.n) padConfirm.value = null;
              },
              builder: (context, ms, _) {
                final inV = Tokens.popCurve.transform((ms / 260).clamp(0, 1));
                final outV = ((ms - 420) / 200).clamp(0.0, 1.0);
                final badge = Tokens.popCurve.transform(((ms - 110) / 280).clamp(0, 1));
                return Opacity(
                  opacity: (inV.clamp(0, 1) * (1 - outV)).toDouble(),
                  child: Transform.scale(
                    scale: (.55 + .45 * inV) * (1 - .06 * outV),
                    child: Column(mainAxisSize: MainAxisSize.min, children: [
                      Container(
                        width: 80,
                        height: 80,
                        decoration: BoxDecoration(
                          shape: BoxShape.circle,
                          color: t.s2,
                          border: Border.all(color: c.ok ? t.accent : t.bad, width: 2),
                          boxShadow: const [BoxShadow(color: Color(0xAA000000), blurRadius: 50, offset: Offset(0, 18), spreadRadius: -20)],
                        ),
                        child: Stack(clipBehavior: Clip.none, alignment: Alignment.center, children: [
                          WIcon(_icon(c.a), size: 36, color: c.ok ? t.bright : t.bad),
                          Positioned(
                            right: -4,
                            bottom: -4,
                            child: Transform.scale(
                              scale: badge,
                              child: Container(
                                width: 30,
                                height: 30,
                                decoration: BoxDecoration(shape: BoxShape.circle, color: c.ok ? t.good : t.bad),
                                child: Center(child: WIcon(c.ok ? 'check' : 'x', size: 18, color: t.onAccent)),
                              ),
                            ),
                          ),
                        ]),
                      ),
                      const SizedBox(height: 10),
                      Container(
                        padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 5),
                        decoration: BoxDecoration(color: fade(t.s2, 6), borderRadius: BorderRadius.circular(999)),
                        child: Text('${c.a['name'] ?? ''}${c.ok ? '' : " — didn't work"}', style: body(context, size: .9375, w: FontWeight.w600, color: t.bright)),
                      ),
                    ]),
                  ),
                );
              },
            ),
          );
        },
      ),
    );
  }
}
