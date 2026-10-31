// The other devices' pointers over the picture (presence.css `.ghost`): a dot in the device's hue
// with its name, gliding where it moves, larger and translucent for a finger, dimmed after 3 s
// still, shrinking away when it leaves.
import 'dart:async';

import 'package:flutter/material.dart';

import '../../../wado/input.dart';
import '../../../wado/presence.dart';
import '../../theme/look.dart';
import '../../theme/tokens.dart';
import '../../widgets/basics.dart' show rem;

class Ghosts extends StatefulWidget {
  const Ghosts({super.key});
  @override
  State<Ghosts> createState() => _GhostsState();
}

class _GhostsState extends State<Ghosts> {
  // Dimming is a clock, not an event: tick once a second.
  late final Timer _t = Timer.periodic(const Duration(seconds: 1), (_) => mounted ? setState(() {}) : null);

  @override
  void dispose() {
    _t.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return IgnorePointer(
      child: ValueListenableBuilder(
        valueListenable: Presence.I.ghosts,
        builder: (context, ghosts, _) {
          final c = Input.I.content;
          return Stack(clipBehavior: Clip.hardEdge, children: [
            for (final g in ghosts.values)
              AnimatedPositioned(
                key: ValueKey(g.id),
                duration: const Duration(milliseconds: 90),
                curve: Tokens.spring,
                left: c.left + g.x * c.width,
                top: c.top + g.y * c.height,
                child: AnimatedOpacity(
                  duration: const Duration(milliseconds: 600),
                  curve: Tokens.spring,
                  opacity: Presence.I.now - g.at > 3000 ? .45 : 1,
                  child: _Ghost(g: g, hue: t.base[const [8, 9, 0xA, 0xB, 0xC, 0xD, 0xE, 0xF][g.hue % 8]]),
                ),
              ),
          ]);
        },
      ),
    );
  }
}

class _Ghost extends StatelessWidget {
  const _Ghost({required this.g, required this.hue});
  final Ghost g;
  final Color hue;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final d = g.touch ? 22.0 : 14.0;
    // Drawn about its own point: the dot's centre is where the pointer is.
    return Transform.translate(
      offset: Offset(-d / 2, -d / 2),
      child: TweenAnimationBuilder<double>(
        tween: Tween(begin: 0, end: 1),
        duration: const Duration(milliseconds: 420),
        curve: Tokens.popCurve,
        builder: (context, v, child) => Row(crossAxisAlignment: CrossAxisAlignment.start, mainAxisSize: MainAxisSize.min, children: [
          Transform.scale(
            scale: v,
            child: Container(
              width: d,
              height: d,
              decoration: BoxDecoration(
                shape: BoxShape.circle,
                color: g.touch ? fade(hue, 45) : hue,
                boxShadow: [BoxShadow(color: fade(hue, 65), spreadRadius: 3)],
              ),
            ),
          ),
          Padding(
            padding: EdgeInsets.only(left: 6 + 4, top: d / 2 + 10 - 7),
            child: Opacity(
              opacity: v.clamp(0, 1),
              child: Transform.scale(
                scale: .8 + .2 * v,
                alignment: Alignment.topLeft,
                child: Container(
                  padding: const EdgeInsets.symmetric(horizontal: 9, vertical: 3),
                  decoration: BoxDecoration(
                    color: hue,
                    borderRadius: BorderRadius.circular(999),
                    boxShadow: const [BoxShadow(color: Color(0x8C000000), blurRadius: 24, offset: Offset(0, 6), spreadRadius: -10)],
                  ),
                  child: Text(g.name, style: TextStyle(fontFamily: Tokens.body, fontSize: rem(context, .7188), fontWeight: FontWeight.w600, color: t.onAccent)),
                ),
              ),
            ),
          ),
        ]),
      ),
    );
  }
}
