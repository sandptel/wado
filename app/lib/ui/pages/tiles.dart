// Quick tiles (port of ui/pages/tiles.rs): which tiles the control centre shows, in order.
import 'package:flutter/material.dart';

import '../../wado/settings.dart';
import '../session/control_centre.dart';
import '../theme/look.dart';
import '../widgets/basics.dart';

class TilesPage extends StatelessWidget {
  const TilesPage({super.key});
  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: Settings.I,
      builder: (context, _) {
        final t = Tok.of(context);
        final on = Settings.I.get<List<String>>('tiles');
        return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          const Why('Tap to add or take away; they show in the order you add them.'),
          const SizedBox(height: 8),
          ListBox(children: [
            for (final e in tileSpecs.entries)
              Press(
                onTap: () => Settings.I.set('tiles', on.contains(e.key) ? on.where((x) => x != e.key).toList() : [...on, e.key]),
                child: Padding(
                  padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 12),
                  child: Row(children: [
                    WIcon(e.value.$1, color: on.contains(e.key) ? t.accent : t.dim),
                    const SizedBox(width: 12),
                    Expanded(child: Text(e.value.$2, style: body(context, color: t.bright))),
                    if (on.contains(e.key)) Text('${on.indexOf(e.key) + 1}', style: mono(context, color: t.accent)),
                  ]),
                ),
              ),
          ]),
        ]);
      },
    );
  }
}
