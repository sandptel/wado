// The first-touch coach (port of pad_coach.js): three scenes on a 7.2 s loop — one finger moves the
// pointer, two scroll, three swipe — until the first touch, then never again on this phone.
import 'package:flutter/material.dart';

import '../../../wado/settings.dart';
import '../../theme/look.dart';
import '../../theme/tokens.dart';
import '../../widgets/basics.dart';

const _scenes = [(1, 'One finger moves the pointer'), (2, 'Two fingers scroll'), (3, 'Three fingers swipe, as on the laptop')];

class PadCoach extends StatefulWidget {
  const PadCoach({super.key});
  @override
  State<PadCoach> createState() => _PadCoachState();
}

class _PadCoachState extends State<PadCoach> with SingleTickerProviderStateMixin {
  late final _loop = AnimationController(vsync: this, duration: const Duration(milliseconds: 7200));
  final bool _seen = Settings.I.b('pad_coach_seen');
  bool _out = false;

  @override
  void initState() {
    super.initState();
    if (!_seen) _loop.repeat();
  }

  @override
  void dispose() {
    _loop.dispose();
    super.dispose();
  }

  void _dismiss() {
    if (_out) return;
    Settings.I.set('pad_coach_seen', true);
    setState(() => _out = true);
  }

  @override
  Widget build(BuildContext context) {
    if (_seen) return const SizedBox.shrink();
    final t = Tok.of(context);
    // Dismissed by any touch on the surface; never takes the touch itself.
    return Listener(
      behavior: HitTestBehavior.translucent,
      onPointerDown: (_) => _dismiss(),
      child: IgnorePointer(
        child: AnimatedOpacity(
          opacity: _out ? 0 : 1,
          duration: const Duration(milliseconds: 240),
          curve: Curves.easeIn,
          child: AnimatedBuilder(
            animation: _loop,
            builder: (context, _) {
              return Stack(children: [
                for (var i = 0; i < 3; i++) _scene(context, i, (_loop.value - i / 3) % 1),
                Positioned(left: 0, right: 0, bottom: 14, child: Text('Touch anywhere to start', textAlign: TextAlign.center, style: body(context, size: .75, color: t.dim))),
              ]);
            },
          ),
        ),
      ),
    );
  }

  /// One scene at `p` of the loop (0..1, its own third first): in by 4%, held to 29%, out by 33%.
  Widget _scene(BuildContext context, int i, double p) {
    final t = Tok.of(context);
    final o = p < .04 ? p / .04 : (p < .29 ? 1.0 : (p < .333 ? 1 - (p - .29) / .043 : 0.0));
    if (o <= 0) return const SizedBox.shrink();
    final (n, words) = _scenes[i];
    final m = Tokens.spring;
    // Fingers' paths (`tpcoach-1/2/3`), as fractions of the loop.
    Offset at(double p) {
      double seg(double a, double b) => m.transform(((p - a) / (b - a)).clamp(0, 1));
      return switch (n) {
        1 => p < .18 ? Offset.lerp(const Offset(0, 22), const Offset(64, -14), seg(.06, .18))! : Offset.lerp(const Offset(64, -14), const Offset(112, 8), seg(.18, .28))!,
        2 => Offset.lerp(const Offset(0, 34), const Offset(0, -24), seg(.06, .26))!,
        _ => Offset.lerp(const Offset(-26, 0), const Offset(44, 0), seg(.06, .26))!,
      };
    }

    final lefts = switch (n) { 1 => [24.0], 2 => [52.0, 96.0], _ => [22.0, 62.0, 102.0] };
    return Center(
      child: Opacity(
        opacity: o,
        child: Transform.translate(
          offset: Offset(0, p < .04 ? 8 * (1 - p / .04) : 0),
          child: Column(mainAxisSize: MainAxisSize.min, children: [
            SizedBox(
              width: 176,
              height: 96,
              child: Stack(clipBehavior: Clip.none, children: [
                for (final l in lefts)
                  Positioned(
                    left: l + at(p).dx,
                    top: 30 + at(p).dy,
                    child: Container(
                      width: 36,
                      height: 36,
                      decoration: BoxDecoration(shape: BoxShape.circle, color: fade(t.accent, 78), border: Border.all(color: t.accent, width: 2)),
                    ),
                  ),
              ]),
            ),
            const SizedBox(height: 16),
            Text(words, textAlign: TextAlign.center, style: body(context, size: 1, w: FontWeight.w600, color: t.bright)),
          ]),
        ),
      ),
    );
  }
}
