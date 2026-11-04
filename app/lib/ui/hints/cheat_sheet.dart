// The ? overlay: what every gesture and control does *here*, laid over the live screen — built
// from this phone's actual settings (three-finger mappings, edge swipes, raw touch, Game mode), so
// it never describes a gesture that is turned off. Tap anywhere to put it away.
import 'package:flutter/material.dart';

import '../../wado/host.dart';
import '../../wado/settings.dart';
import '../../wado/ux.dart';
import '../theme/look.dart';
import '../theme/tokens.dart';
import '../widgets/basics.dart';

/// (fingers, icon, words) for [surface] (`home`, `picture`, `pad`) on a screen of [size] — the
/// shared rows (`wado_ux::help`), so the web's ? says the same thing for the same settings.
List<(int, String, String)> gestures(String surface, Size size) {
  final s = Settings.I;
  final c = Host.I.config.value;
  final rows = ux('help', {
    'surface': surface,
    'form': ux('form', {'w': size.width, 'h': size.height, 'touch': true}),
    'raw': s.s('touch_mode') == 'touch',
    'swipe3': s.b('swipe3'),
    'edge_swipe': s.b('edge_swipe'),
    'game': s.overlaid,
    'gestures': c?['gestures'] ?? const {},
    'binds': c?['binds'] ?? const {},
    'bind_mod': c?['bind_mod'] ?? '',
  });
  return [
    for (final r in (rows as List? ?? const []))
      if (r is Map) ((r['fingers'] as num? ?? 0).toInt(), '${r['icon'] ?? ''}', '${r['words'] ?? ''}'),
  ];
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
    final list = gestures(surface, MediaQuery.sizeOf(context));
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
