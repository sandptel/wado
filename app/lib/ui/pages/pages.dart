// Settings pages pushed over home or a computer's page (port of ui/subpage.rs + ui/pages/*):
// one dispatcher, a page each.
import 'package:flutter/material.dart';

import '../../wado/nav.dart';
import '../theme/look.dart';
import '../widgets/basics.dart';
import 'page_registry.dart';

class SettingsPage extends StatelessWidget {
  const SettingsPage({super.key, required this.computerKey, required this.page});
  final String? computerKey;
  final String page;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final mq = MediaQuery.of(context);
    final p = pageFor(page);
    return Column(children: [
      Container(
        height: 56 + mq.padding.top,
        padding: EdgeInsets.fromLTRB(8, mq.padding.top, 8, 0),
        decoration: BoxDecoration(color: t.s0, border: Border(bottom: BorderSide(color: t.line))),
        child: Row(children: [
          IconBtn('left', label: 'Back', color: t.bright, onTap: Nav.I.back),
          const SizedBox(width: 4),
          Expanded(child: Text(p.title, maxLines: 1, overflow: TextOverflow.ellipsis, style: display(context, rem(context, 1.0625)))),
        ]),
      ),
      Expanded(
        child: p.fullBleed
            ? p.build(context)
            : ListView(
                padding: EdgeInsets.fromLTRB(16, 12, 16, 32 + mq.padding.bottom),
                children: [Center(child: ConstrainedBox(constraints: const BoxConstraints(maxWidth: 520), child: p.build(context)))],
              ),
      ),
    ]);
  }
}
