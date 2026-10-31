// Quick tiles (port of ui/pages/tiles.rs): which tiles the control centre shows, in order — the
// shown ones with up, down and remove, the rest with add, and a way back to the default eight.
import 'package:flutter/material.dart';

import '../../wado/settings.dart';
import '../session/cc/tiles.dart';
import '../theme/look.dart';
import '../theme/tokens.dart';
import '../widgets/basics.dart';

class TilesPage extends StatelessWidget {
  const TilesPage({super.key});
  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: Settings.I,
      builder: (context, _) {
        final t = Tok.of(context);
        final pool = tilePool((_) {});
        final byId = {for (final x in pool) x.id: x};
        final list = shownTiles();
        final spare = pool.where((x) => !list.contains(x.id)).toList();
        void put(List<String> l) => Settings.I.set('tiles', l);
        Widget btn(String icon, String label, VoidCallback? tap) => Press(
              onTap: tap,
              label: label,
              child: SizedBox(width: 40, height: 40, child: Center(child: WIcon(icon, color: tap == null ? fade(t.dim, 60) : t.dim))),
            );
        Widget row(TileDef d, List<Widget> acts) => ConstrainedBox(
              constraints: const BoxConstraints(minHeight: 48),
              child: Row(children: [
                Disc(child: WIcon(d.icon, color: t.accent)),
                const SizedBox(width: 10),
                Expanded(child: Text(d.label, style: body(context, w: FontWeight.w600, color: t.bright))),
                ...acts,
              ]),
            );
        Widget head(String s) => Text(s, style: body(context, w: FontWeight.w600, color: t.bright));
        return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          WCard(children: [
            head('Shown'),
            for (var i = 0; i < list.length; i++)
              row(byId[list[i]]!, [
                btn('up', 'Move up', i == 0 ? null : () => put([...list]..[i] = list[i - 1]..[i - 1] = list[i])),
                btn('down', 'Move down', i + 1 == list.length ? null : () => put([...list]..[i] = list[i + 1]..[i + 1] = list[i])),
                btn('x', 'Remove', () => put([...list]..removeAt(i))),
              ]),
          ]),
          if (spare.isNotEmpty) ...[
            const SizedBox(height: 12),
            WCard(children: [
              head('More tiles'),
              for (final d in spare) row(d, [btn('plus', 'Add', () => put([...list, d.id]))]),
            ]),
          ],
          const SizedBox(height: 12),
          Btn('Reset to the default eight', onTap: () => put(tileDefaults)),
        ]);
      },
    );
  }
}
