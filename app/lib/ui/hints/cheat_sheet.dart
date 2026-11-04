// The ? overlay: what every gesture and control does *here*, laid over the live screen — built
// from this phone's actual settings (three-finger mappings, edge swipes, raw touch, Game mode), so
// it never describes a gesture that is turned off. Tap anywhere to put it away.
import 'package:flutter/material.dart';

import '../../wado/host.dart';
import '../../wado/settings.dart';
import '../theme/look.dart';
import '../theme/tokens.dart';
import '../widgets/basics.dart';

String _action(String a) => switch (a) {
      'app-drawer' => 'your apps',
      'control-centre' => 'the controls',
      'back' => 'Back',
      'focus-next' => 'the next window',
      'keyboard' => 'the keyboard',
      'workspace-next' => 'the next workspace',
      'workspace-prev' => 'the last workspace',
      _ => a.replaceAll('-', ' '),
    };

/// (fingers, arrow icon or '', words) for [surface]: `picture` or `pad`.
List<(int, String, String)> gestures(String surface) {
  final s = Settings.I;
  final raw = s.s('touch_mode') == 'touch';
  final out = <(int, String, String)>[];
  if (surface == 'home') {
    return const [
      (1, '', 'Tap a game to play it, ready the way you set it up'),
      (1, 'pin', 'Hold a game for its Game mode settings, or to turn it off'),
      (1, 'plus', 'Add a game: turn on Game mode for any app'),
      (1, 'play', '▶ on a computer starts or rejoins its desktop'),
      (1, 'down', 'The name at the top switches computers, or shows them all'),
      (1, 'camera', 'Scan a code to add a computer'),
    ];
  }
  if (surface == 'pad') {
    out.add((0, 'pad', 'The buttons press the computer\'s gamepad. ⋯ changes the layout'));
  }
  if (!raw) {
    out
      ..add((1, '', 'Tap to click'))
      ..add((1, 'down', 'Drag one finger to scroll'))
      ..add((1, 'pin', 'Hold for a right-click, or to drag'))
      ..add((2, 'max', 'Pinch with two fingers to zoom'));
  } else {
    out.add((1, '', 'Touches go to the app as touches'));
  }
  if (s.b('swipe3')) {
    final custom = Host.I.config.value?['gestures'];
    const dflt = {'up': 'app-drawer', 'down': 'control-centre', 'left': 'back', 'right': 'focus-next'};
    for (final d in const ['up', 'down', 'left', 'right']) {
      final a = custom is Map && custom['swipe-3-$d'] != null ? '${custom['swipe-3-$d']}' : dflt[d]!;
      if (a != 'none') out.add((3, d, 'Three fingers $d: ${_action(a)}'));
    }
  }
  if (s.b('edge_swipe')) out.add((1, 'right', 'Swipe in from a side edge: the next workspace'));
  if (s.overlaid && !s.b('swipe3')) out.add((0, 'lock', 'Game mode has the swipes off, so a game never catches one by accident'));
  return out;
}

Future<void> showCheatSheet(BuildContext context, String surface) => Navigator.of(context, rootNavigator: true).push(
      PageRouteBuilder<void>(
        opaque: false,
        barrierDismissible: true,
        barrierLabel: 'Close the help',
        transitionDuration: Tok.of(context).dur,
        reverseTransitionDuration: Tok.of(context).dur,
        pageBuilder: (ctx, a, _) => _Sheet(surface: surface),
        transitionsBuilder: (ctx, a, _, child) => FadeTransition(opacity: CurvedAnimation(parent: a, curve: Tokens.spring), child: child),
      ),
    );

class _Sheet extends StatelessWidget {
  const _Sheet({required this.surface});
  final String surface;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final list = gestures(surface);
    return GestureDetector(
      behavior: HitTestBehavior.opaque,
      onTap: () => Navigator.of(context).pop(),
      child: ColoredBox(
        color: fade(t.base[0], 18),
        child: SafeArea(
          child: Center(
            child: ConstrainedBox(
              constraints: const BoxConstraints(maxWidth: 420),
              child: ListView(shrinkWrap: true, padding: const EdgeInsets.all(20), children: [
                Text('What you can do here', style: display(context, 22, ls: -.4, color: t.bright)),
                const SizedBox(height: 4),
                Text('Tap anywhere to close. It\'s always under ?.', style: body(context, size: .75, color: t.dim)),
                const SizedBox(height: 16),
                for (final (n, icon, words) in list)
                  Padding(
                    padding: const EdgeInsets.only(bottom: 8),
                    child: Container(
                      padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
                      decoration: BoxDecoration(color: t.s2, borderRadius: BorderRadius.circular(t.r2), border: Border.all(color: fade(t.accent, 40), width: 1.5)),
                      child: Row(children: [
                        SizedBox(width: 56, child: _Fingers(n: n, icon: icon)),
                        const SizedBox(width: 10),
                        Expanded(child: Text(words, style: body(context, size: .8125, color: t.bright))),
                      ]),
                    ),
                  ),
              ]),
            ),
          ),
        ),
      ),
    );
  }
}

/// Ghost fingers: n dots, and the way they move.
class _Fingers extends StatelessWidget {
  const _Fingers({required this.n, required this.icon});
  final int n;
  final String icon;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return Row(mainAxisSize: MainAxisSize.min, children: [
      for (var i = 0; i < n; i++)
        Container(
          width: 10,
          height: 10,
          margin: const EdgeInsets.only(right: 3),
          decoration: BoxDecoration(color: fade(t.accent, 25), shape: BoxShape.circle, boxShadow: [BoxShadow(color: fade(t.accent, 78), spreadRadius: 3)]),
        ),
      if (icon.isNotEmpty) WIcon(icon, size: 16, color: t.accent),
    ]);
  }
}
